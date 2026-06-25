use audiopus::{
    coder::{Decoder, Encoder},
    packet::Packet,
    Application, Bitrate, Channels, MutSignals, SampleRate,
};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::collections::VecDeque;
use std::convert::TryFrom;
use std::sync::{Arc, Mutex, OnceLock};

const SAMPLE_RATE: u32 = 48000;
const FRAME_SIZE: usize = 960; // 20ms at 48kHz
const MAX_PLAYBACK_BUF: usize = 48000;
/// How long open-mic transmission should continue after the level briefly
/// dips below the threshold. 12 × 20ms ~= 240ms, enough to avoid chopping
/// quiet syllables/word middles without leaving the mic open for too long.
const GATE_HANGOVER_FRAMES: usize = 12;

/// ALSA/JACK can print handled probe failures directly to native stderr.
/// Set SLPAUTH_AUDIO_DEBUG=1 to leave backend stderr untouched.
#[cfg(unix)]
fn with_native_audio_stderr_suppressed<T>(f: impl FnOnce() -> T) -> T {
    use std::os::fd::AsRawFd;

    if std::env::var_os("SLPAUTH_AUDIO_DEBUG").is_some() {
        return f();
    }

    static STDERR_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _lock = STDERR_LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();

    unsafe {
        let saved = libc::dup(libc::STDERR_FILENO);
        if saved < 0 {
            return f();
        }

        let null = match std::fs::OpenOptions::new().write(true).open("/dev/null") {
            Ok(file) => file,
            Err(_) => {
                libc::close(saved);
                return f();
            }
        };

        if libc::dup2(null.as_raw_fd(), libc::STDERR_FILENO) < 0 {
            libc::close(saved);
            return f();
        }

        struct RestoreStderr(libc::c_int);
        impl Drop for RestoreStderr {
            fn drop(&mut self) {
                unsafe {
                    libc::dup2(self.0, libc::STDERR_FILENO);
                    libc::close(self.0);
                }
            }
        }

        let _restore = RestoreStderr(saved);
        f()
    }
}

#[cfg(not(unix))]
fn with_native_audio_stderr_suppressed<T>(f: impl FnOnce() -> T) -> T {
    f()
}

/// Shared state for voice audio capture and playback.
pub struct VoiceEngine {
    /// Encoded Opus packets ready to be sent to peers.
    outgoing: Arc<Mutex<Vec<Vec<u8>>>>,
    /// Whether the PTT key is currently held.
    ptt_active: Arc<Mutex<bool>>,
    /// Whether local mic capture is unmuted.
    enabled: Arc<Mutex<bool>>,
    /// Audio playback buffer (device-format samples are generated from this f32 buffer).
    playback_buf: Arc<Mutex<VecDeque<f32>>>,
    /// Opus decoder for incoming packets.
    decoder: Arc<Mutex<Decoder>>,
    /// Output device channel count (for upmixing mono).
    out_channels: u16,
    /// Output device sample rate.
    out_rate: u32,
    /// Playback volume multiplier (0.0 – 5.0).
    volume: Arc<Mutex<f32>>,
    /// Noise gate threshold (0.0 – 1.0). Frames below this RMS are dropped in open-mic mode.
    threshold: Arc<Mutex<f32>>,
    /// When true, the PTT key must be held to transmit. When false, voice is open-mic.
    use_ptt: Arc<Mutex<bool>>,
    /// True when we are actively transmitting (for talking border indicator).
    self_talking: Arc<Mutex<bool>>,
    /// Current microphone input RMS level (0.0 – 1.0+), updated by capture callback.
    input_level: Arc<Mutex<f32>>,
    /// Set of peer IDs that sent voice data recently (for talking border).
    peers_talking: Arc<Mutex<std::collections::HashMap<String, std::time::Instant>>>,
    input_status: String,
    output_status: String,
    // Keep streams alive.
    _input_stream: Option<cpal::Stream>,
    _output_stream: Option<cpal::Stream>,
}

#[derive(Clone)]
pub struct VoiceWorkerHandle {
    outgoing: Arc<Mutex<Vec<Vec<u8>>>>,
    playback_buf: Arc<Mutex<VecDeque<f32>>>,
    decoder: Arc<Mutex<Decoder>>,
    out_channels: u16,
    out_rate: u32,
    peers_talking: Arc<Mutex<std::collections::HashMap<String, std::time::Instant>>>,
}

impl VoiceWorkerHandle {
    pub fn drain_outgoing(&self) -> Vec<Vec<u8>> {
        let mut buf = self.outgoing.lock().unwrap();
        buf.drain(..).collect()
    }

    pub fn play_incoming(&self, opus_data: &[u8], peer_id: &str) {
        self.peers_talking
            .lock()
            .unwrap()
            .insert(peer_id.to_string(), std::time::Instant::now());

        let packet = match Packet::try_from(opus_data) {
            Ok(p) => p,
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("[voice] Invalid Opus packet ({} bytes): {:?}", opus_data.len(), _e);
                return;
            }
        };
        let mut pcm = vec![0f32; FRAME_SIZE];
        let signals = match MutSignals::try_from(&mut pcm[..]) {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut dec = self.decoder.lock().unwrap();
        match dec.decode_float(Some(packet), signals, false) {
            Ok(samples) => {
                pcm.truncate(samples);

                let resampled = if self.out_rate != SAMPLE_RATE {
                    resample(&pcm, SAMPLE_RATE, self.out_rate)
                } else {
                    pcm
                };

                let mut out = Vec::with_capacity(resampled.len() * self.out_channels as usize);
                for s in &resampled {
                    for _ in 0..self.out_channels {
                        out.push(*s);
                    }
                }

                self.playback_buf.lock().unwrap().extend(out.iter());
            }
            Err(_e) => {
                #[cfg(debug_assertions)]
                eprintln!("Decode error: {_e}");
            }
        }
    }
}

impl VoiceEngine {
    /// List available audio input device names (cached to avoid ALSA/JACK reprobe spam).
    pub fn input_device_names() -> Vec<String> {
        static CACHED: OnceLock<Vec<String>> = OnceLock::new();
        CACHED
            .get_or_init(|| {
                let host = with_native_audio_stderr_suppressed(cpal::default_host);
                match with_native_audio_stderr_suppressed(|| host.input_devices()) {
                    Ok(devices) => devices
                        .filter_map(|d| with_native_audio_stderr_suppressed(|| d.name()).ok())
                        .collect(),
                    Err(_) => Vec::new(),
                }
            })
            .clone()
    }

    /// List available audio output device names (cached to avoid ALSA/JACK reprobe spam).
    pub fn output_device_names() -> Vec<String> {
        static CACHED: OnceLock<Vec<String>> = OnceLock::new();
        CACHED
            .get_or_init(|| {
                let host = with_native_audio_stderr_suppressed(cpal::default_host);
                match with_native_audio_stderr_suppressed(|| host.output_devices()) {
                    Ok(devices) => devices
                        .filter_map(|d| with_native_audio_stderr_suppressed(|| d.name()).ok())
                        .collect(),
                    Err(_) => Vec::new(),
                }
            })
            .clone()
    }

    /// Create a VoiceEngine using specific device names.
    /// Empty string means system default.
    pub fn new_with_devices(input_device: &str, output_device: &str) -> Self {
        let host = with_native_audio_stderr_suppressed(cpal::default_host);

        let outgoing: Arc<Mutex<Vec<Vec<u8>>>> = Arc::new(Mutex::new(Vec::new()));
        let ptt_active: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        // Start muted; the chat mic button is the local mute/unmute control.
        let enabled: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let use_ptt: Arc<Mutex<bool>> = Arc::new(Mutex::new(true));
        let threshold: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.01));
        let self_talking: Arc<Mutex<bool>> = Arc::new(Mutex::new(false));
        let input_level: Arc<Mutex<f32>> = Arc::new(Mutex::new(0.0));

        let input_device = select_input_device(&host, input_device);
        let mut input_status = "input stream: no input device".to_string();
        let input_stream = input_device.and_then(|device| {
            let device_name = with_native_audio_stderr_suppressed(|| device.name()).unwrap_or_else(|_| "unknown input".to_string());
            let supported = match with_native_audio_stderr_suppressed(|| device.default_input_config()) {
                Ok(config) => config,
                Err(err) => {
                    input_status = format!("input stream: no default config for {device_name}: {err}");
                    return None;
                }
            };
            let sample_format = supported.sample_format();
            let config: cpal::StreamConfig = supported.into();
            let stream = match sample_format {
                SampleFormat::F32 => build_input_stream::<f32>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::I16 => build_input_stream::<i16>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::U16 => build_input_stream::<u16>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::I8 => build_input_stream::<i8>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::U8 => build_input_stream::<u8>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::I32 => build_input_stream::<i32>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::U32 => build_input_stream::<u32>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::I64 => build_input_stream::<i64>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::U64 => build_input_stream::<u64>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                SampleFormat::F64 => build_input_stream::<f64>(
                    &device,
                    &config,
                    outgoing.clone(),
                    ptt_active.clone(),
                    enabled.clone(),
                    use_ptt.clone(),
                    threshold.clone(),
                    self_talking.clone(),
                    input_level.clone(),
                ),
                _ => None,
            };
            if stream.is_some() {
                input_status = format!(
                    "input stream: ok ({device_name}, {sample_format}, {} Hz, {} ch)",
                    config.sample_rate.0,
                    config.channels,
                );
            } else {
                input_status = format!(
                    "input stream: failed ({device_name}, {sample_format}, {} Hz, {} ch)",
                    config.sample_rate.0,
                    config.channels,
                );
            }
            stream
        });

        let playback_buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let volume: Arc<Mutex<f32>> = Arc::new(Mutex::new(1.0));

        let output_device = select_output_device(&host, output_device);
        let mut output_status = "output stream: no output device".to_string();
        let (out_channels, out_rate, output_stream) = output_device
            .and_then(|device| {
                let device_name = with_native_audio_stderr_suppressed(|| device.name()).unwrap_or_else(|_| "unknown output".to_string());
                let supported = match with_native_audio_stderr_suppressed(|| device.default_output_config()) {
                    Ok(config) => config,
                    Err(err) => {
                        output_status = format!("output stream: no default config for {device_name}: {err}");
                        return None;
                    }
                };
                let dev_ch = supported.channels();
                let dev_rate = supported.sample_rate().0;
                let sample_format = supported.sample_format();
                let config: cpal::StreamConfig = supported.into();
                let stream = match sample_format {
                    SampleFormat::F32 => build_output_stream::<f32>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::I16 => build_output_stream::<i16>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::U16 => build_output_stream::<u16>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::I8 => build_output_stream::<i8>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::U8 => build_output_stream::<u8>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::I32 => build_output_stream::<i32>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::U32 => build_output_stream::<u32>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::I64 => build_output_stream::<i64>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::U64 => build_output_stream::<u64>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    SampleFormat::F64 => build_output_stream::<f64>(
                        &device,
                        &config,
                        playback_buf.clone(),
                        volume.clone(),
                    ),
                    _ => None,
                };
                if stream.is_some() {
                    output_status = format!(
                        "output stream: ok ({device_name}, {sample_format}, {} Hz, {} ch)",
                        config.sample_rate.0,
                        config.channels,
                    );
                    Some((dev_ch, dev_rate, stream))
                } else {
                    output_status = format!(
                        "output stream: failed ({device_name}, {sample_format}, {} Hz, {} ch)",
                        config.sample_rate.0,
                        config.channels,
                    );
                    None
                }
            })
            .unwrap_or((2, 48000, None));

        let decoder = Arc::new(Mutex::new(
            Decoder::new(SampleRate::Hz48000, Channels::Mono)
                .expect("Failed to create Opus decoder"),
        ));

        VoiceEngine {
            outgoing,
            ptt_active,
            enabled,
            playback_buf,
            decoder,
            out_channels,
            out_rate,
            volume,
            threshold,
            use_ptt,
            self_talking,
            input_level,
            peers_talking: Arc::new(Mutex::new(std::collections::HashMap::new())),
            input_status,
            output_status,
            _input_stream: input_stream,
            _output_stream: output_stream,
        }
    }

    /// Take all pending encoded voice packets (to send to peers).
    pub fn drain_outgoing(&self) -> Vec<Vec<u8>> {
        let mut buf = self.outgoing.lock().unwrap();
        buf.drain(..).collect()
    }

    /// Decode an incoming Opus packet and queue for playback.
    /// `peer_id` is used to track which peer is talking.
    pub fn play_incoming(&self, opus_data: &[u8], peer_id: &str) {
        self.worker_handle().play_incoming(opus_data, peer_id);
    }

    pub fn worker_handle(&self) -> VoiceWorkerHandle {
        VoiceWorkerHandle {
            outgoing: self.outgoing.clone(),
            playback_buf: self.playback_buf.clone(),
            decoder: self.decoder.clone(),
            out_channels: self.out_channels,
            out_rate: self.out_rate,
            peers_talking: self.peers_talking.clone(),
        }
    }

    pub fn set_ptt(&self, active: bool) {
        *self.ptt_active.lock().unwrap() = active;
    }

    pub fn ptt_active(&self) -> bool {
        *self.ptt_active.lock().unwrap()
    }

    pub fn input_status(&self) -> &str {
        &self.input_status
    }

    pub fn output_status(&self) -> &str {
        &self.output_status
    }

    pub fn queued_outgoing_packets(&self) -> usize {
        self.outgoing.lock().unwrap().len()
    }

    pub fn playback_queued_samples(&self) -> usize {
        self.playback_buf.lock().unwrap().len()
    }

    pub fn set_enabled(&self, enabled: bool) {
        *self.enabled.lock().unwrap() = enabled;
    }

    pub fn is_enabled(&self) -> bool {
        *self.enabled.lock().unwrap()
    }

    pub fn set_volume(&self, vol: f32) {
        *self.volume.lock().unwrap() = vol.clamp(0.0, 5.0);
    }

    pub fn set_threshold(&self, t: f32) {
        *self.threshold.lock().unwrap() = t.clamp(0.0, 1.0);
    }

    pub fn set_use_ptt(&self, use_ptt: bool) {
        *self.use_ptt.lock().unwrap() = use_ptt;
    }

    pub fn input_level(&self) -> f32 {
        *self.input_level.lock().unwrap()
    }

    pub fn is_self_talking(&self) -> bool {
        *self.self_talking.lock().unwrap()
    }

    pub fn is_peer_talking(&self, peer_id: &str) -> bool {
        let map = self.peers_talking.lock().unwrap();
        if let Some(last) = map.get(peer_id) {
            last.elapsed().as_millis() < 300
        } else {
            false
        }
    }

}

fn select_input_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    if name.is_empty() {
        with_native_audio_stderr_suppressed(|| host.default_input_device())
    } else {
        with_native_audio_stderr_suppressed(|| host.input_devices())
            .ok()?
            .find(|d| with_native_audio_stderr_suppressed(|| d.name()).ok().as_deref() == Some(name))
            .or_else(|| with_native_audio_stderr_suppressed(|| host.default_input_device()))
    }
}

fn select_output_device(host: &cpal::Host, name: &str) -> Option<cpal::Device> {
    if name.is_empty() {
        with_native_audio_stderr_suppressed(|| host.default_output_device())
    } else {
        with_native_audio_stderr_suppressed(|| host.output_devices())
            .ok()?
            .find(|d| with_native_audio_stderr_suppressed(|| d.name()).ok().as_deref() == Some(name))
            .or_else(|| with_native_audio_stderr_suppressed(|| host.default_output_device()))
    }
}

fn build_input_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    outgoing: Arc<Mutex<Vec<Vec<u8>>>>,
    ptt_active: Arc<Mutex<bool>>,
    enabled: Arc<Mutex<bool>>,
    use_ptt: Arc<Mutex<bool>>,
    threshold: Arc<Mutex<f32>>,
    self_talking: Arc<Mutex<bool>>,
    input_level: Arc<Mutex<f32>>,
) -> Option<cpal::Stream>
where
    T: Sample + SizedSample + Send + 'static,
    f32: FromSample<T>,
{
    let dev_ch = config.channels;
    let dev_rate = config.sample_rate.0;

    let mut enc = Encoder::new(SampleRate::Hz48000, Channels::Mono, Application::Voip).ok()?;
    enc.set_bitrate(Bitrate::BitsPerSecond(64000)).ok();
    let encoder = Arc::new(Mutex::new(enc));
    let sample_buf: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
    let mut smoothed_rms = 0.0f32;
    let mut gate_hold_frames = 0usize;

    let stream = with_native_audio_stderr_suppressed(|| {
        device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !*enabled.lock().unwrap() {
                    sample_buf.lock().unwrap().clear();
                    smoothed_rms = 0.0;
                    gate_hold_frames = 0;
                    *self_talking.lock().unwrap() = false;
                    return;
                }

                let is_ptt_mode = *use_ptt.lock().unwrap();
                let ptt_held = *ptt_active.lock().unwrap();
                if is_ptt_mode && !ptt_held {
                    sample_buf.lock().unwrap().clear();
                    smoothed_rms = 0.0;
                    gate_hold_frames = 0;
                    *self_talking.lock().unwrap() = false;
                    return;
                }

                let mono: Vec<f32> = data
                    .chunks(dev_ch as usize)
                    .map(|ch| {
                        ch.iter()
                            .map(|sample| sample.to_sample::<f32>())
                            .sum::<f32>()
                            / dev_ch as f32
                    })
                    .collect();

                let resampled = if dev_rate != SAMPLE_RATE {
                    resample(&mono, dev_rate, SAMPLE_RATE)
                } else {
                    mono
                };

                let mut buf = sample_buf.lock().unwrap();
                buf.extend(resampled.iter());
                let thresh = *threshold.lock().unwrap();

                while buf.len() >= FRAME_SIZE {
                    let frame: Vec<f32> = buf.drain(..FRAME_SIZE).collect();
                    let rms = (frame.iter().map(|s| s * s).sum::<f32>() / frame.len() as f32).sqrt();
                    *input_level.lock().unwrap() = rms;

                    // Smooth the gate detector and add a short hangover. This keeps
                    // words from being chopped when the speaker briefly dips below
                    // the threshold in the middle of a syllable/word.
                    if rms > smoothed_rms {
                        smoothed_rms = smoothed_rms * 0.40 + rms * 0.60;
                    } else {
                        smoothed_rms = smoothed_rms * 0.85 + rms * 0.15;
                    }

                    // In open-mic mode apply the smoothed noise gate; in PTT mode trust user intent.
                    if !is_ptt_mode {
                        if smoothed_rms >= thresh {
                            gate_hold_frames = GATE_HANGOVER_FRAMES;
                        } else if gate_hold_frames > 0 {
                            gate_hold_frames -= 1;
                        } else {
                            *self_talking.lock().unwrap() = false;
                            continue;
                        }
                    }

                    *self_talking.lock().unwrap() = true;
                    let mut output = vec![0u8; 4000];
                    let enc = encoder.lock().unwrap();
                    if let Ok(len) = enc.encode_float(&frame, &mut output) {
                        output.truncate(len);
                        outgoing.lock().unwrap().push(output);
                    }
                }
            },
            |_err| {
                #[cfg(debug_assertions)]
                eprintln!("Input stream error: {_err}");
            },
            None,
        )
    })
    .ok()?;

    with_native_audio_stderr_suppressed(|| stream.play()).ok()?;
    Some(stream)
}

fn build_output_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    playback_buf: Arc<Mutex<VecDeque<f32>>>,
    volume: Arc<Mutex<f32>>,
) -> Option<cpal::Stream>
where
    T: Sample + SizedSample + FromSample<f32> + Send + 'static,
{
    let dev_ch = config.channels;
    let stream = with_native_audio_stderr_suppressed(|| {
        device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut buf = playback_buf.lock().unwrap();
                let vol = *volume.lock().unwrap();
                let max_buf = MAX_PLAYBACK_BUF * dev_ch as usize;
                if buf.len() > max_buf {
                    let excess = buf.len() - max_buf / 2;
                    buf.drain(..excess);
                }
                for sample in data.iter_mut() {
                    let value = (buf.pop_front().unwrap_or(0.0) * vol).clamp(-1.0, 1.0);
                    *sample = T::from_sample(value);
                }
            },
            |_err| {
                #[cfg(debug_assertions)]
                eprintln!("Output stream error: {_err}");
            },
            None,
        )
    })
    .ok()?;

    with_native_audio_stderr_suppressed(|| stream.play()).ok()?;
    Some(stream)
}

fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((input.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let a = input[idx.min(input.len() - 1)];
        let b = input[(idx + 1).min(input.len() - 1)];
        output.push(a + (b - a) * frac as f32);
    }
    output
}

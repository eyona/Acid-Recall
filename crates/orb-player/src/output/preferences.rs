//! Device selection and ownership recovery happen outside the audio callback.

use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{BufferSize, SampleFormat, StreamConfig, SupportedBufferSize};
use serde::{Deserialize, Serialize};

use super::{Audio, CallbackOutput, Error, Health, Output, Stream, build, err};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Empty follows the platform's default host (WASAPI on Windows).
    pub backend: String,
    /// Stable CPAL device ID; absent follows this host's default output.
    pub device: Option<String>,
    pub sample_rate: Option<u32>,
    pub buffer_frames: Option<u32>,
    /// Zero-based start of a stereo pair (0 = outputs 1/2).
    pub output_pair: u16,
}

impl Config {
    pub fn sanitize(&mut self) {
        self.sample_rate = self.sample_rate.filter(|r| (8000..=384000).contains(r));
        self.buffer_frames = self.buffer_frames.filter(|n| (16..=32768).contains(n));
        self.output_pair = self.output_pair.min(254) & !1;
        self.device = self.device.take().filter(|s| !s.is_empty());
    }
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub rates: Vec<u32>,
    pub channels: u16,
    pub buffer_range: Option<(u32, u32)>,
}

#[derive(Clone, Debug, Default)]
pub struct Catalog {
    pub backend: String,
    pub devices: Vec<DeviceInfo>,
    pub default_device: Option<String>,
}

struct Devices {
    info: Catalog,
    handles: Vec<cpal::Device>,
}

pub fn backends() -> Vec<String> {
    cpal::available_hosts()
        .into_iter()
        .map(|h| h.name().to_owned())
        .collect()
}

pub fn default_backend() -> String {
    cpal::default_host().id().name().into()
}

fn host_name(name: &str) -> Result<cpal::HostId, Error> {
    if name.is_empty() {
        return Ok(cpal::default_host().id());
    }
    cpal::available_hosts()
        .into_iter()
        .find(|h| h.name() == name)
        .ok_or_else(|| Error(format!("Audio backend {name} is unavailable in this build")))
}

fn scan(name: &str) -> Result<Devices, Error> {
    let id = host_name(name)?;
    let host = cpal::host_from_id(id).map_err(|e| err("opening the audio backend", e))?;
    let default_device = host
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|i| i.to_string());
    let mut result = Devices {
        info: Catalog {
            backend: id.name().into(),
            default_device,
            devices: Vec::new(),
        },
        handles: Vec::new(),
    };
    for device in host
        .output_devices()
        .map_err(|e| err("listing audio outputs", e))?
    {
        let Ok(ranges) = device
            .supported_output_configs()
            .map(Iterator::collect::<Vec<_>>)
        else {
            continue;
        };
        let Ok(device_id) = device.id() else { continue };
        let name = device
            .description()
            .map(|d| d.name().to_owned())
            .unwrap_or_else(|_| device_id.to_string());
        let mut rates: Vec<u32> = [
            8000, 11025, 16000, 22050, 32000, 44100, 48000, 88200, 96000, 176400, 192000, 352800,
            384000,
        ]
        .into_iter()
        .filter(|r| {
            ranges
                .iter()
                .any(|c| c.min_sample_rate() <= *r && *r <= c.max_sample_rate())
        })
        .collect();
        if let Ok(default) = device.default_output_config()
            && default.sample_rate() > 0
        {
            rates.push(default.sample_rate());
        }
        rates.sort_unstable();
        rates.dedup();
        let channels = ranges.iter().map(|c| c.channels()).max().unwrap_or(0);
        let buffer_range = ranges
            .iter()
            .filter_map(|c| match c.buffer_size() {
                SupportedBufferSize::Range { min, max } => Some((*min, *max)),
                SupportedBufferSize::Unknown => None,
            })
            .next();
        result.info.devices.push(DeviceInfo {
            id: device_id.to_string(),
            name,
            rates,
            channels,
            buffer_range,
        });
        result.handles.push(device);
    }
    Ok(result)
}

fn select(device: &cpal::Device, options: &Config) -> Result<(StreamConfig, SampleFormat), Error> {
    let ranges: Vec<_> = device
        .supported_output_configs()
        .map_err(|e| err("audio capabilities", e))?
        .collect();
    let default_rate = device
        .default_output_config()
        .map_or(48000, |c| c.sample_rate());
    select_ranges(&ranges, default_rate, options)
}

fn select_ranges(
    ranges: &[cpal::SupportedStreamConfigRange],
    default_rate: u32,
    options: &Config,
) -> Result<(StreamConfig, SampleFormat), Error> {
    let rate = options.sample_rate.unwrap_or_else(|| {
        if ranges
            .iter()
            .any(|r| r.min_sample_rate() <= 44100 && r.max_sample_rate() >= 44100)
        {
            44100
        } else {
            default_rate
        }
    });
    let required = options.output_pair.saturating_add(2);
    let mono = options.output_pair == 0 && ranges.iter().all(|r| r.channels() == 1);
    let range = ranges
        .iter()
        .filter(|r| {
            matches!(
                r.sample_format(),
                SampleFormat::F32
                    | SampleFormat::F64
                    | SampleFormat::I8
                    | SampleFormat::I16
                    | SampleFormat::I24
                    | SampleFormat::I32
                    | SampleFormat::I64
                    | SampleFormat::U8
                    | SampleFormat::U16
                    | SampleFormat::U24
                    | SampleFormat::U32
                    | SampleFormat::U64
            ) && r.min_sample_rate() <= rate
                && rate <= r.max_sample_rate()
                && (r.channels() >= required || mono)
                && options.buffer_frames.is_none_or(|n| match r.buffer_size() {
                    SupportedBufferSize::Range { min, max } => (*min..=*max).contains(&n),
                    SupportedBufferSize::Unknown => true,
                })
        })
        .max_by_key(|r| {
            (
                r.channels() == required,
                r.sample_format() == SampleFormat::F32,
                r.sample_format() == SampleFormat::F64,
                std::cmp::Reverse(r.channels()),
            )
        })
        .ok_or_else(|| {
            Error(
                "This output pair, sample rate or buffer size is not supported. \
            Try Driver default."
                    .into(),
            )
        })?;
    let format = range.sample_format();
    let mut config = (*range).with_sample_rate(rate).config();
    config.buffer_size = options
        .buffer_frames
        .map_or(BufferSize::Default, BufferSize::Fixed);
    Ok((config, format))
}

/// Owns the existing engine across stream changes. A callback's destructor returns its complete
/// Output through a bounded queue; only the UI drops resampler buffers and opens new drivers.
pub struct Session {
    stream: Option<Stream>,
    returned: Option<rtrb::Consumer<Output>>,
    audio: Option<Audio>,
    catalogs: Vec<Devices>,
    active: Option<Config>,
}

impl Session {
    pub fn new(audio: Audio) -> Self {
        Self {
            stream: None,
            returned: None,
            audio: Some(audio),
            catalogs: Vec::new(),
            active: None,
        }
    }

    pub fn stream(&self) -> Option<&Stream> {
        self.stream.as_ref()
    }
    pub fn config(&self) -> Option<&Config> {
        self.active.as_ref()
    }

    pub fn catalog(&self, backend: &str) -> Option<&Catalog> {
        let name = host_name(backend).ok()?.name();
        self.catalogs
            .iter()
            .find(|c| c.info.backend == name)
            .map(|c| &c.info)
    }

    pub fn poll(&mut self) {
        if self.audio.is_none()
            && let Some(queue) = &mut self.returned
            && let Ok(output) = queue.pop()
        {
            self.audio = Some(output.into_audio());
            self.returned = None;
        }
    }

    fn stop(&mut self) -> Result<(), Error> {
        drop(self.stream.take());
        self.poll();
        let audio = self.audio.as_mut().ok_or_else(|| {
            Error("The driver is still releasing its stream. Please retry.".into())
        })?;
        audio.prepare_device_change();
        Ok(())
    }

    fn load_catalog(&mut self, backend: &str) -> Result<(), Error> {
        let scanned = scan(backend)?;
        self.catalogs
            .retain(|c| c.info.backend != scanned.info.backend);
        self.catalogs.push(scanned);
        Ok(())
    }

    /// ASIO metadata queries briefly load drivers, so scan only while our stream is closed.
    pub fn refresh(&mut self, backend: &str) -> Result<(), Error> {
        let previous = self.active.clone();
        self.stop()?;
        let result = self.load_catalog(backend);
        let restore = previous.as_ref().map(|c| self.open(c)).transpose();
        result?;
        restore?;
        Ok(())
    }

    pub fn apply(&mut self, options: &Config) -> Result<(), Error> {
        let previous = self.active.clone();
        self.stop()?;
        let result = self.open(options);
        if let Err(error) = result {
            self.poll();
            if let Some(old) = previous
                && let Err(restore) = self.open(&old)
            {
                return Err(Error(format!(
                    "{error}\nPrevious output could not reopen: {restore}"
                )));
            }
            return Err(error);
        }
        Ok(())
    }

    fn open(&mut self, options: &Config) -> Result<(), Error> {
        if self.catalog(&options.backend).is_none() {
            self.load_catalog(&options.backend)?;
        }
        let backend = host_name(&options.backend)?.name();
        let devices = self
            .catalogs
            .iter()
            .find(|c| c.info.backend == backend)
            .unwrap();
        let selected = options
            .device
            .as_ref()
            .or(devices.info.default_device.as_ref());
        let index = devices
            .info
            .devices
            .iter()
            .position(|d| Some(&d.id) == selected)
            .ok_or_else(|| {
                Error(
                    "The selected audio output is unavailable. \
                Refresh devices or choose another output."
                        .into(),
                )
            })?;
        let device = &devices.handles[index];
        let (config, format) = select(device, options)?;
        let output = Output::prepare(
            &mut self.audio,
            config.channels as usize,
            config.sample_rate,
            options.output_pair as usize,
        )?;
        let (sender, receiver) = rtrb::RingBuffer::new(1);
        self.returned = Some(receiver);
        let health = Health::started();
        let callback = CallbackOutput {
            output: Some(output),
            returned: Some(sender),
        };
        let stream = match format {
            SampleFormat::F32 => build::<f32>(device, config, callback, health.clone()),
            SampleFormat::F64 => build::<f64>(device, config, callback, health.clone()),
            SampleFormat::I8 => build::<i8>(device, config, callback, health.clone()),
            SampleFormat::I16 => build::<i16>(device, config, callback, health.clone()),
            SampleFormat::I24 => build::<cpal::I24>(device, config, callback, health.clone()),
            SampleFormat::I32 => build::<i32>(device, config, callback, health.clone()),
            SampleFormat::I64 => build::<i64>(device, config, callback, health.clone()),
            SampleFormat::U8 => build::<u8>(device, config, callback, health.clone()),
            SampleFormat::U16 => build::<u16>(device, config, callback, health.clone()),
            SampleFormat::U24 => build::<cpal::U24>(device, config, callback, health.clone()),
            SampleFormat::U32 => build::<u32>(device, config, callback, health.clone()),
            SampleFormat::U64 => build::<u64>(device, config, callback, health.clone()),
            f => {
                drop(callback);
                Err(Error(format!("Unsupported output sample format {f}")))
            }
        }?;
        use cpal::traits::StreamTrait;
        stream
            .play()
            .map_err(|e| err("starting the selected output", e))?;
        self.stream = Some(Stream {
            _stream: stream,
            backend: backend.into(),
            device: devices.info.devices[index].name.clone(),
            config,
            format,
            resampled: config.sample_rate != 44100,
            health,
        });
        self.active = Some(options.clone());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(all(windows, target_arch = "x86_64"))]
    fn windows_x64_build_exposes_asio_and_wasapi_backends() {
        let hosts = backends();
        assert!(hosts.iter().any(|h| h == "ASIO"));
        assert!(hosts.iter().any(|h| h == "WASAPI"));
    }

    #[test]
    #[ignore = "requires a real audio output device"]
    fn real_device_reopen_and_bad_driver_restore_keep_the_engine() {
        let (_, audio) =
            crate::Player::new(std::sync::Arc::new(orb_engine::tables::Tables::new()), None);
        let mut session = Session::new(audio);
        session.apply(&Config::default()).unwrap();
        let device = session.stream().unwrap().device.clone();
        assert!(
            session
                .apply(&Config {
                    backend: "not-an-audio-backend".into(),
                    ..Config::default()
                })
                .is_err()
        );
        assert_eq!(session.stream().unwrap().device, device);
        session.refresh("").unwrap();
        assert_eq!(session.stream().unwrap().device, device);
        session.stop().unwrap();
        assert!(session.audio.is_some());
    }

    #[test]
    fn selected_rate_stereo_pair_and_buffer_must_be_supported_together() {
        let ranges = [
            cpal::SupportedStreamConfigRange::new(
                2,
                44100,
                44100,
                SupportedBufferSize::Range { min: 32, max: 512 },
                SampleFormat::F32,
            ),
            cpal::SupportedStreamConfigRange::new(
                4,
                48000,
                48000,
                SupportedBufferSize::Range { min: 64, max: 1024 },
                SampleFormat::I24,
            ),
        ];
        let options = Config {
            sample_rate: Some(48000),
            output_pair: 2,
            buffer_frames: Some(128),
            ..Config::default()
        };
        let (chosen, format) = select_ranges(&ranges, 48000, &options).unwrap();
        assert_eq!(chosen.channels, 4);
        assert_eq!(chosen.sample_rate, 48000);
        assert_eq!(chosen.buffer_size, BufferSize::Fixed(128));
        assert_eq!(format, SampleFormat::I24);
        assert!(
            select_ranges(
                &ranges,
                48000,
                &Config {
                    sample_rate: Some(44100),
                    ..options.clone()
                }
            )
            .is_err()
        );
        assert!(
            select_ranges(
                &ranges,
                48000,
                &Config {
                    buffer_frames: Some(4096),
                    ..options
                }
            )
            .is_err()
        );
        assert_eq!(
            select_ranges(&ranges, 48000, &Config::default())
                .unwrap()
                .0
                .sample_rate,
            44100
        );
    }
}

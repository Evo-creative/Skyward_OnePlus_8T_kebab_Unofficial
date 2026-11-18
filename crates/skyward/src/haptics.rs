use crate::waveform::{made, share, ticks, RATE, TICKS_RATE};
use anyhow::{anyhow, ensure, Context, Result};
use evdev::raw_stream::RawDevice;
use skyward_config::Strength;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// linux/input-event-codes.h
const EV_FF: u16 = 0x15;
const FF_PERIODIC: u16 = 0x51;
const FF_CUSTOM: u16 = 0x5d;

/// The ticks play at this share of the drive the driver gives samples at in
/// full, whatever the slider says: the 17 Ultra's own system plays them at
/// 3527 mV of the 8500 mV its device tree gives.
const TICKS_STRENGTH: f32 = 0.415;
/// How long the ticks stay loaded once their samples have run out, as the
/// phone's own system leaves them: the motor comes to rest on its own
/// before taking them away brakes it.
const TICKS_REST: Duration = Duration::from_millis(40);

/// The vibration motor, played through the kernel's force-feedback
/// events. The driver fills its queue with a made click's samples as the
/// click is loaded, refilling it for no second play, and keeps the click
/// playing after its samples have run out, even when told to stop, until
/// the click is taken away. So each click is loaded as it is played, at the
/// strength the slider gives, and taken away once it has played out.
pub(crate) struct Haptics {
    motor: Motor,
}

/// How the phone's kernel drives the motor.
enum Motor {
    /// Through the kernel's force-feedback events.
    Force(Force),
    /// As a light: a duration, then a word that starts it, one clip at a
    /// time.
    Led(Led),
}

impl Haptics {
    /// The motor this phone's configuration describes, in the form its
    /// kernel drives it.
    pub fn open(cfg: &skyward_config::Haptics) -> Result<Haptics> {
        let strength = cfg.strength.unwrap_or(skyward_config::Strength::DEFAULT);
        let motor = match &cfg.led {
            Some(dir) => Motor::Led(Led::open(dir, strength)?),
            None => Motor::Force(Force::open(cfg)?),
        };
        Ok(Haptics { motor })
    }

    /// One short click at the strength the slider's `position` gives.
    /// Gives how long it plays.
    pub fn click(&mut self, position: f32) -> Result<Duration> {
        match &mut self.motor {
            Motor::Force(force) => force.click(position),
            Motor::Led(led) => led.click(position),
        }
    }

    /// The ticks of the screen going off. Gives how long they play.
    pub fn ticks(&mut self) -> Result<Duration> {
        match &mut self.motor {
            Motor::Force(force) => force.ticks(),
            Motor::Led(led) => led.ticks(),
        }
    }
}

/// A motor the kernel's force-feedback events drive.
struct Force {
    dev: File,
    click: Click,
    ticks: Click,
    strength: Strength,
    /// The click playing, by its slot and its count, until it is taken
    /// away; a click taken away early by the next is not taken away again.
    playing: Arc<Mutex<Option<(i16, u64)>>>,
    clicks: u64,
}

/// What a click is.
enum Click {
    /// A waveform the driver keeps, by its number. The driver says how long
    /// it plays as it loads it.
    Kept(i16),
    /// Samples made at the motor's resonant frequency, which the driver
    /// plays as they are, at `rate` a second.
    Samples { data: Vec<i8>, rate: u32 },
}

impl Force {
    /// The force-feedback device and the configured click. Loading a click
    /// fills the driver's queue with what it plays, so the click is loaded
    /// only as it is played.
    fn open(cfg: &skyward_config::Haptics) -> Result<Force> {
        let (path, dev) = motor()?;
        let file = cfg.resonance.as_deref().context("the configuration does not say where the motor's resonant frequency is read")?;
        let strength = cfg.strength.unwrap_or(skyward_config::Strength::DEFAULT);
        let hz = resonance(file)?;
        let click = match cfg.click {
            Some(id) => {
                eprintln!("haptics {}: clicks with waveform {id}, ticks made for a motor swinging at {hz} Hz", path.display());
                Click::Kept(id)
            }
            None => {
                eprintln!("haptics {}: clicks and ticks made for a motor swinging at {hz} Hz", path.display());
                Click::Samples { data: made(hz), rate: RATE }
            }
        };
        let ticks = Click::Samples { data: ticks(hz), rate: TICKS_RATE };
        Ok(Force { dev, click, ticks, strength, playing: Arc::new(Mutex::new(None)), clicks: 0 })
    }

    /// One short click at the strength the slider's `position` gives, after
    /// taking away one still playing. Gives how long it plays.
    fn click(&mut self, position: f32) -> Result<Duration> {
        self.start(false, share(&self.strength, position))
    }

    /// The ticks of the screen going off, as strong as the phone's own
    /// system plays them whatever the slider says. Gives how long they play.
    fn ticks(&mut self) -> Result<Duration> {
        self.start(true, TICKS_STRENGTH)
    }

    /// Play the ticks, or else the click, at `share` of its full strength,
    /// after taking away what still plays.
    fn start(&mut self, ticks: bool, share: f32) -> Result<Duration> {
        let mut playing = self.playing.lock().expect("no thread panics while it holds the click playing");
        if let Some((slot, _)) = playing.take() {
            erase(&self.dev, slot).context("take away the click still playing")?;
        }
        let effect = if ticks { &self.ticks } else { &self.click };
        let loaded = upload(&self.dev, effect, share).context("load the click")?;
        play(&mut self.dev, loaded.slot).context("play the click")?;
        self.clicks += 1;
        let this = (loaded.slot, self.clicks);
        *playing = Some(this);
        drop(playing);
        let dev = self.dev.try_clone().context("share the motor")?;
        let length = loaded.length + if ticks { TICKS_REST } else { Duration::ZERO };
        // Timed from now: a thread may wait a while before it first runs.
        let until = Instant::now() + length;
        let playing = self.playing.clone();
        std::thread::spawn(move || {
            std::thread::sleep(until.saturating_duration_since(Instant::now()));
            let mut playing = playing.lock().expect("no thread panics while it holds the click playing");
            if *playing == Some(this) {
                *playing = None;
                if let Err(e) = erase(&dev, this.0) {
                    eprintln!("take away the click: {e}");
                }
            }
        });
        Ok(length)
    }
}

/// A motor the kernel shows as a light, as the phone's own vibrator service
/// drives it: a file that says how long the next vibration lasts, one that
/// starts it, and one the motor's strength is set by. The driver plays one
/// clip at a time, and the clip it picks grows with the duration asked for,
/// so a click is the shortest it has.
struct Led {
    dir: PathBuf,
    strength: Strength,
}

impl Led {
    /// How long a click plays, as the phone's own system plays one.
    const CLICK_MS: u32 = 10;
    /// How long each of the two ticks of the screen going off plays, and
    /// how long after the first the second comes: the driver plays one clip
    /// at a time, and a clip takes a moment to end.
    const TICK_MS: u32 = 10;
    const TICK_GAP: Duration = Duration::from_millis(60);
    /// The most the driver takes, and the least that still moves the motor.
    const FULL_GAIN: f32 = 0x75 as f32;
    const LEAST_GAIN: f32 = 0x01 as f32;

    fn open(dir: &str, strength: Strength) -> Result<Led> {
        let dir = PathBuf::from(dir);
        ensure!(dir.join("activate").is_file(), "{} holds no activate file", dir.display());
        Ok(Led { dir, strength })
    }

    fn write(&self, file: &str, value: &str) -> Result<()> {
        let path = self.dir.join(file);
        fs::write(&path, value).with_context(|| format!("write {value} to {}", path.display()))
    }

    /// A share of the full strength as the number the driver takes: what the
    /// slider's ends are set to, rising with it as the click's does, and
    /// never so low that the motor does not move.
    fn gain(&self, share: f32) -> Result<()> {
        let value = (share.clamp(0.0, 1.0) * Self::FULL_GAIN).round().clamp(Self::LEAST_GAIN, Self::FULL_GAIN) as u32;
        self.write("gain", &format!("{value:#x}"))
    }

    /// Whether the driver still has a clip playing, which its `activate`
    /// file then ignores: the AW8697's check comes before it reads the value.
    fn still_playing(&self) -> bool {
        fs::read_to_string(self.dir.join("rtp_going")).is_ok_and(|s| s.trim() == "1")
    }

    /// Stop what still plays. The AW8697's `cont` file stops it whatever the
    /// value is, and resets the chip where it will not come to rest; its
    /// `activate` file alone cannot, as that returns before it stops.
    fn stop(&self) -> Result<()> {
        if self.dir.join("cont").is_file() {
            self.write("cont", "0")?;
        }
        Ok(())
    }

    /// Play one clip of `ms` at `share` of the full strength, as the phone's
    /// own service plays one: the state first, the duration, then the word
    /// that starts it.
    fn play(&self, ms: u32, share: f32) -> Result<()> {
        // A clip the driver still thinks is playing takes the whole start
        // for itself, as one left behind when the recovery came out of
        // fastbootd does; end it first.
        if self.still_playing() {
            eprintln!("haptics {}: a clip still plays; stopping it first", self.dir.display());
            self.stop()?;
        }
        self.write("state", "1")?;
        self.gain(share)?;
        self.write("duration", &ms.to_string())?;
        self.write("activate", "1")
    }

    fn click(&mut self, position: f32) -> Result<Duration> {
        self.play(Self::CLICK_MS, share(&self.strength, position))?;
        Ok(Duration::from_millis(u64::from(Self::CLICK_MS)))
    }

    fn ticks(&mut self) -> Result<Duration> {
        self.play(Self::TICK_MS, TICKS_STRENGTH)?;
        // The second tick comes once the first has ended, from a thread of
        // its own so the screen's loop does not wait for it.
        let led = Led { dir: self.dir.clone(), strength: self.strength };
        std::thread::spawn(move || {
            std::thread::sleep(Self::TICK_GAP);
            if let Err(e) = led.play(Self::TICK_MS, TICKS_STRENGTH) {
                eprintln!("play the second tick: {e:#}");
            }
        });
        Ok(Self::TICK_GAP + Duration::from_millis(u64::from(Self::TICK_MS)))
    }
}

/// Play the configured click once at the slider's `position`, or else the
/// ticks of the screen going off, saying how strongly and for how long, for
/// trying out a motor.
pub fn probe(cfg: &skyward_config::Haptics, position: f32, ticks: bool) -> Result<()> {
    let mut haptics = Haptics::open(cfg)?;
    let (what, strength, length) = if ticks {
        ("ticks", TICKS_STRENGTH, haptics.ticks()?)
    } else {
        ("click", share(&cfg.strength.unwrap_or(skyward_config::Strength::DEFAULT), position), haptics.click(position)?)
    };
    // Once it has played out, what played goes with the device as the
    // command ends.
    std::thread::sleep(length);
    println!("played the {what} at {strength:.3} of the full strength, {} ms long", length.as_millis());
    Ok(())
}

/// The first input device under /dev/input that takes periodic
/// force-feedback effects, opened for writing.
fn motor() -> Result<(PathBuf, File)> {
    for (path, dev) in evdev::raw_stream::enumerate() {
        if takes_periodic(&dev) {
            let file = OpenOptions::new().read(true).write(true).open(&path).with_context(|| format!("open {}", path.display()))?;
            return Ok((path, file));
        }
    }
    Err(anyhow!("no input device takes force-feedback effects"))
}

fn takes_periodic(dev: &RawDevice) -> bool {
    dev.supported_ff().is_some_and(|ff| ff.contains(evdev::FFEffectCode::FF_PERIODIC))
}

/// The motor's resonant frequency in hertz, from the file the driver says
/// it in, in tenths of a hertz.
fn resonance(file: &str) -> Result<f32> {
    let text = fs::read_to_string(file).with_context(|| format!("read {file}"))?;
    let tenths: u32 = text.split_whitespace().next().and_then(|n| n.parse().ok()).with_context(|| format!("{file} holds {text:?}"))?;
    ensure!(tenths > 0, "{file} says the motor swings at 0 Hz");
    Ok(tenths as f32 / 10.0)
}

/// A click as the driver loaded it: the slot the kernel keeps it in, and
/// how long it plays.
struct Loaded {
    slot: i16,
    length: Duration,
}

/// Samples handed to the driver to play as they are, as its
/// `custom_fifo_data` lays them out.
#[repr(C)]
struct Samples {
    /// Unused by the driver.
    index: u32,
    length: u32,
    rate: u32,
    data: *const i8,
}

/// Load `click` at `share` of its full strength as a custom periodic
/// effect in a slot of its own. The driver drives the motor at the click's
/// highest voltage times the magnitude.
fn upload(dev: &File, click: &Click, share: f32) -> Result<Loaded> {
    // The driver tells the two kinds apart by the length in bytes of what
    // the effect points to, and loads nothing for any other length, though
    // the upload still succeeds. For a waveform it keeps it reads the
    // waveform's number from the first of three values and writes how long
    // it plays into the other two, in whole seconds and then milliseconds.
    let mut kept: [i16; 3] = [0; 3];
    let mut samples = Samples { index: 0, length: 0, rate: RATE, data: std::ptr::null() };
    let (len, data) = match click {
        Click::Kept(id) => {
            kept[0] = *id;
            (std::mem::size_of_val(&kept), kept.as_mut_ptr())
        }
        Click::Samples { data, rate } => {
            samples.length = u32::try_from(data.len())?;
            samples.rate = *rate;
            samples.data = data.as_ptr();
            (std::mem::size_of::<Samples>(), (&mut samples as *mut Samples).cast::<i16>())
        }
    };
    let periodic = libc::ff_periodic_effect {
        waveform: FF_CUSTOM,
        period: 0,
        magnitude: (share.clamp(0.0, 1.0) * f32::from(i16::MAX)).round() as i16,
        offset: 0,
        phase: 0,
        envelope: libc::ff_envelope { attack_length: 0, attack_level: 0, fade_length: 0, fade_level: 0 },
        custom_len: len as u32,
        custom_data: data,
    };
    let mut effect = libc::ff_effect {
        type_: FF_PERIODIC,
        id: -1,
        direction: 0,
        trigger: libc::ff_trigger { button: 0, interval: 0 },
        replay: libc::ff_replay { length: 0, delay: 0 },
        u: [0; 4],
    };
    // The union holds the periodic effect; it is exactly as large.
    const _: () = assert!(std::mem::size_of::<libc::ff_periodic_effect>() == std::mem::size_of::<[u64; 4]>());
    effect.u = unsafe { std::mem::transmute::<libc::ff_periodic_effect, [u64; 4]>(periodic) };
    // EVIOCSFF: _IOW('E', 0x80, struct ff_effect)
    let size = std::mem::size_of::<libc::ff_effect>() as u32;
    let request = (1u32 << 30) | (size << 16) | (u32::from(b'E') << 8) | 0x80;
    if unsafe { libc::ioctl(dev.as_raw_fd(), request as _, &mut effect) } < 0 {
        return Err(io::Error::last_os_error().into());
    }
    let length = match click {
        Click::Kept(id) => {
            let [_, seconds, millis] = kept;
            let length = Duration::from_secs(u64::try_from(seconds)?) + Duration::from_millis(u64::try_from(millis)?);
            ensure!(!length.is_zero(), "the driver loaded waveform {id} without saying how long it plays");
            length
        }
        // The queue is full before the samples start playing.
        Click::Samples { data, rate } => Duration::from_secs_f64(data.len() as f64 / f64::from(*rate)),
    };
    Ok(Loaded { slot: effect.id, length })
}

/// Start the loaded effect in slot `id`.
fn play(dev: &mut File, id: i16) -> io::Result<()> {
    let event = libc::input_event { time: libc::timeval { tv_sec: 0, tv_usec: 0 }, type_: EV_FF, code: id as u16, value: 1 };
    let bytes = unsafe { std::slice::from_raw_parts((&event as *const libc::input_event).cast::<u8>(), std::mem::size_of::<libc::input_event>()) };
    dev.write_all(bytes)
}

/// Take the effect in slot `id` away, which stops it for good.
fn erase(dev: &File, id: i16) -> io::Result<()> {
    // EVIOCRMFF: _IOW('E', 0x81, int)
    let request = (1u32 << 30) | ((std::mem::size_of::<libc::c_int>() as u32) << 16) | (u32::from(b'E') << 8) | 0x81;
    if unsafe { libc::ioctl(dev.as_raw_fd(), request as _, libc::c_int::from(id)) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

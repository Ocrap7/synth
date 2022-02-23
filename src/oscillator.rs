use std::{
    fs::OpenOptions,
    sync::{Arc, Mutex},
};

use modulator::sources::{
    Newtonian, ScalarGoalFollower, ScalarSpring, ShiftRegister, ShiftRegisterInterp, Wave,
};
use modulator::{Modulator, ModulatorEnv};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Sample, SampleFormat, Stream, StreamConfig,
};


pub trait SoundWriter {
    fn next_sample(&mut self) -> f32;
}

pub enum OscillatorShape {
    Sine,
    Triangle,
    Sawtooth,
    Square,
}

pub struct Oscillator {
    config: StreamConfig,
    shape: OscillatorShape,
    pub frequency: f32,
    volume: f32,
    sample_clock: f32,
    freq_mod: Option<Arc<Mutex<Box<Oscillator>>>>,
    output: bool,
    offset: f32,

    time: u64,
    // wave: Wave,
}

impl Oscillator {
    pub fn new(config: StreamConfig, shape: OscillatorShape, frequency: f32) -> Oscillator {
        Oscillator {
            config,
            shape,
            frequency,
            volume: 0.2,
            sample_clock: 0.0,
            freq_mod: None,
            output: false,
            offset: 0.0,
            time: 0,
            // wave: Wave::new(1.0, 100.0),
        }
    }

    pub fn output<T>(ss: Arc<Mutex<Box<Oscillator>>>, device: &cpal::Device) -> Stream
    where
        T: Sample,
    {
        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
        ss.lock().as_mut().unwrap().output = true;

        let config = ss.lock().as_mut().unwrap().config.clone();
        device
            .build_output_stream(
                &config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    write_data(data, config.channels as _, ss.clone());
                },
                err_fn,
            )
            .unwrap()
    }

    pub fn use_freq_mod(&mut self, osc: Arc<Mutex<Box<Oscillator>>>) {
        self.freq_mod = Some(osc)
    }

    pub fn set_volume(&mut self, volume: f32) {
        self.volume = volume
    }

    pub fn sample_rate(&self) -> f32 {
        self.config.sample_rate.0 as _
    }
}

impl SoundWriter for Oscillator {
    fn next_sample(&mut self) -> f32 {
        let mut s = 0.0;
        match &self.freq_mod {
            Some(f) => {
                let mut osc = f.lock();
                let osc = osc.as_mut().unwrap();
                s = osc.next_sample();

                self.sample_clock = self.sample_clock + 1.0;

                if self.sample_clock >= self.sample_rate() / osc.frequency {
                    self.sample_clock = 0.0;
                }
            }
            _ => {
                self.sample_clock = self.sample_clock + 1.0;
                if self.sample_clock >= self.sample_rate() / self.frequency {
                    self.sample_clock = 0.0;
                }
            }
        };

        let sample_clock = self.sample_clock;

        let bias = 2.0 * std::f32::consts::PI / self.sample_rate();

        match self.shape {
            OscillatorShape::Sine => {
                let value = if self.output {
                    (self.frequency * sample_clock * bias + s).sin()
                } else {
                    (sample_clock * self.frequency * bias).sin()
                };

                value * self.volume
            }
            OscillatorShape::Sawtooth => {
                // let value = (sample_clock % (self.config.sample_rate.0 as f32 / freq)
                //     / self.config.sample_rate.0 as f32)
                //     * freq
                //     - 0.5;
                // let value = (sample_clock * self.config.sample_rate.0 as f32) * freq - 0.5;
                // value * self.volume / 2.0
                // let mut sum = 0.0;
                // for i in 1..100 {
                //     sum += -((self.frequency * sample_clock * bias + s) * i as f32).sin()
                //         / 2.0
                //         / i as f32
                // }
                // sum * self.volume
                let value =
                    (2.0 * self.frequency / self.sample_rate() * sample_clock + s) % 2.0 - 1.0;
                value * self.volume
            }
            OscillatorShape::Triangle => {
                let value = if self.output {
                    (self.frequency * sample_clock * bias + s).sin().asin() * 0.62
                } else {
                    (sample_clock * self.frequency * bias).sin().asin()
                };

                value * self.volume
            }
            OscillatorShape::Square => {
                let value = 1.0
                    / (100.0f32.powf(100.0 * (sample_clock * self.frequency * bias + s).sin())
                        + 1.0)
                    - 0.5;

                value * self.volume
            }
            _ => 0.0,
        }
    }
}

fn write_data<T, SW>(output: &mut [T], channels: usize, sw: Arc<Mutex<Box<SW>>>)
where
    T: cpal::Sample,
    SW: SoundWriter,
{
    for frame in output.chunks_mut(channels) {
        let p = &sw.lock().as_mut().unwrap().next_sample();
        let value: T = cpal::Sample::from::<f32>(p);
        for sample in frame.iter_mut() {
            *sample = value;
        }
    }
}


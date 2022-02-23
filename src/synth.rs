use std::{
    collections::{HashMap, VecDeque},
    sync::{mpsc::Receiver, Arc, Mutex},
};

use cpal::{
    traits::{DeviceTrait, StreamTrait},
    Sample, Stream,
};
use fundsp::hacker::*;
use midi_control::{MidiMessage, MidiNote};

use crate::util::midi_key_to_freq;

pub trait SoundWriter {
    fn next_sample(sw: Arc<Mutex<Self>>) -> (f64, f64);
}

pub struct Synth<'a> {
    voices: Vec<Voice<'a>>,
    next_voice: VecDeque<u8>,
    key_map: HashMap<MidiNote, u8>,
    sample_rate: f64,
}

impl Synth<'static> {
    pub fn new(sample_rate: f64) -> Synth<'static> {
        Synth {
            voices: vec![
                Voice::new(sample_rate),
                Voice::new(sample_rate),
                Voice::new(sample_rate),
                Voice::new(sample_rate),
            ],
            next_voice: VecDeque::from(vec![0, 1, 2, 3]),
            key_map: HashMap::default(),
            sample_rate,
        }
    }

    pub fn output<T>(
        synth: Arc<Mutex<Self>>,
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        recv: Receiver<MidiMessage>,
    ) -> Stream
    where
        T: Sample,
    {
        let init_synth = synth.clone();
        let mut ssynth = init_synth.lock();
        for voice in &mut ssynth.as_mut().unwrap().voices {
            voice.init();
        }

        let stream_synth = synth.clone();

        let err_fn = |err| eprintln!("an error occurred on stream: {}", err);
        let channels = config.channels as usize;
        let stream = device
            .build_output_stream(
                config,
                move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                    write_data(data, channels as _, stream_synth.clone());
                },
                err_fn,
            )
            .unwrap();

        let ssynth = synth.clone();
        std::thread::spawn(move || loop {
            if let Ok(msg) = recv.recv() {
                match msg {
                    MidiMessage::NoteOn(c, k) => {
                        let mut synth = ssynth.lock();
                        let synth = synth.as_mut().unwrap();

                        if let Some(i) = synth.next_voice.pop_front() {
                            let thing = &mut synth.voices[i as usize];
                            *thing.frequency.lock().unwrap() =
                                midi_key_to_freq::<f64>(k.key) / 2.0f64;
                            *thing.volume.lock().unwrap() = 0.6;

                            synth.key_map.insert(k.key, i);
                        }
                    }
                    MidiMessage::NoteOff(c, k) => {
                        let mut synth = ssynth.lock();
                        let synth = synth.as_mut().unwrap();

                        if let Some(i) = synth.key_map.get(&k.key) {
                            let i = *i;
                            let thing = &mut synth.voices[i as usize];
                            *thing.volume.lock().unwrap() = 0.0;
                            synth.next_voice.push_back(i);
                        }
                    }
                    MidiMessage::Invalid => (),
                    _ => println!("{:?}", msg),
                }
            }
        });
        stream
    }
}

impl<'a> SoundWriter for Synth<'_> {
    fn next_sample(sw: Arc<Mutex<Self>>) -> (f64, f64) {
        let mut sw = sw.lock();
        let sw = sw.as_mut().unwrap();

        let value = sw.voices.iter_mut().fold((0.0, 0.0), |value, voice| {
            let voice = voice.get_value();
            (value.0 + voice.0, value.1 + voice.1)
        });

        value
    }
}

type Oscillator<'a> = An<
    Pipe<
        f64,
        Pipe<
            f64,
            Envelope<f64, f64, Box<dyn Fn(f64) -> f64 + Send + Sync>, f64>,
            WaveSynth<'a, f64, U1>,
        >,
        FixedSvf<f64, f64, LowpassMode<f64>>,
    >,
>;

pub struct Voice<'a> {
    oscillators: Vec<Oscillator<'a>>,
    frequency: Arc<Mutex<f64>>,
    volume: Arc<Mutex<f64>>,
    sample_rate: f64,
}

impl Voice<'_> {
    pub fn new(sample_rate: f64) -> Voice<'static> {
        Voice {
            oscillators: Vec::new(),
            frequency: Arc::new(Mutex::new(100.0)),
            volume: Arc::new(Mutex::new(0.0)),
            sample_rate,
        }
    }

    pub fn init(&mut self) {
        for i in 0..3 {
            let osc_freq = self.frequency.clone();
            let osc_vol = self.volume.clone();

            let osc_cb: Box<dyn Fn(f64) -> f64 + Send + Sync> = Box::new(move |_| {
                (*osc_freq.lock().unwrap() + ((i as f64 - 1.0) * 0.5)) * *osc_vol.lock().unwrap()
            });
            let osc = lfo(osc_cb) >> saw() >> lowpass_hz(500.0, 1.0);
            self.oscillators.push(osc);
        }
    }

    fn get_value(&mut self) -> (f64, f64) {
        self.oscillators
            .iter_mut()
            .fold((0.0, 0.0), |value, voice| {
                let voice = voice.get_stereo();
                (value.0 + voice.0, value.1 + voice.1)
            })
    }
}

fn write_data<T, SW>(output: &mut [T], channels: usize, next_sample: Arc<Mutex<SW>>)
where
    T: cpal::Sample,
    SW: SoundWriter,
{
    for frame in output.chunks_mut(channels) {
        let sample = SoundWriter::next_sample(next_sample.clone());
        let left: T = cpal::Sample::from::<f32>(&(sample.0 as f32));
        let right: T = cpal::Sample::from::<f32>(&(sample.1 as f32));

        for (channel, sample) in frame.iter_mut().enumerate() {
            if channel & 1 == 0 {
                *sample = left;
            } else {
                *sample = right;
            }
        }
    }
}

#![feature(type_name_of_val)]
use std::{
    io,
    sync::{mpsc::channel, Arc, Mutex},
};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fundsp::hacker::*;

use gfx_lib::{
    skia_safe::{self, Point, Rect},
    widget::View,
};
use gfx_lib_macro::view_object;

mod oscillator;
mod synth;
mod util;

#[view_object]
struct Graph {
    #[state]
    data_points: Vec<f32>,
}

fn map(value: f32, istart: f32, istop: f32, ostart: f32, ostop: f32) -> f32 {
    ostart + (ostop - ostart) * ((value - istart) / (istop - istart))
}

impl View for Graph {
    fn view(&mut self) -> Option<Arc<Mutex<dyn View>>> {
        None
    }

    fn draw(
        &mut self,
        env: &gfx_lib::widget::Env,
        canvas: &mut skia_safe::Canvas,
        positioning: &gfx_lib::widget::ContentBox,
    ) -> skia_safe::Rect {
        let x = 40.0;
        canvas.draw_rect(
            Rect {
                left: x,
                right: x + 2.0,
                top: 20.0,
                bottom: env.windowed_context.window().inner_size().height as f32 - 40.0,
            },
            &skia_safe::Paint::new(skia_safe::colors::BLACK, None),
        );
        let width = env.windowed_context.window().inner_size().width as f32 - 80.0;

        canvas.draw_rect(
            Rect {
                left: x,
                right: x + width,
                top: env.windowed_context.window().inner_size().height as f32 - 42.0,
                bottom: env.windowed_context.window().inner_size().height as f32 - 40.0,
            },
            &skia_safe::Paint::new(skia_safe::colors::BLACK, None),
        );

        let mut last = (0.0f32, 0.0f32);

        for point in self.data_points.iter().enumerate() {
            let current = (
                map(
                    point.0 as f32,
                    0.0,
                    self.data_points.len() as _,
                    x + 1.0,
                    x + width,
                ),
                map(
                    *point.1,
                    -1.0,
                    1.0,
                    env.windowed_context.window().inner_size().height as f32 - 42.0,
                    20.0,
                ),
            );
            if point.0 > 0 {
                canvas.draw_line(
                    Point {
                        x: last.0,
                        y: last.1,
                    },
                    Point {
                        x: current.0,
                        y: current.1,
                    },
                    &skia_safe::Paint::new(skia_safe::colors::RED, None).set_stroke_width(2.0),
                );
            }
            last = current;
        }

        positioning.content
    }
}

use lazy_static::lazy_static;
use midi_control::MidiMessage;
use midir::MidiIO;

lazy_static! {
    pub static ref GRAPH: Arc<Mutex<Graph>> = { Arc::new(Mutex::new(Graph::new(vec![]))) };
}

fn main() -> io::Result<()> {
    let host = cpal::default_host();
    let device = host.default_output_device().unwrap();
    println!("Device {}", device.name().unwrap());

    let config = device.default_output_config().unwrap();
    println!("Default output config: {:?}", config);

    std::thread::spawn(move || match config.sample_format() {
        cpal::SampleFormat::F32 => run::<f32>(&device, &config.into()),
        cpal::SampleFormat::I16 => run::<i16>(&device, &config.into()),
        cpal::SampleFormat::U16 => run::<u16>(&device, &config.into()),
    });

    gfx_lib::start(GRAPH.clone());
    Ok(())
}

const DEVICE: &str = "Boutique";
fn find_port<T>(midi_io: &T) -> Option<T::Port>
where
    T: midir::MidiIO,
{
    for port in midi_io.ports() {
        if let Ok(port_name) = midi_io.port_name(&port) {
            println!("Port: {}", port_name);
            if port_name.contains(DEVICE) {
                println!("Device: {}", port_name);
                return Some(port);
            }
        }
    }
    None
}

pub fn run<T>(device: &cpal::Device, config: &cpal::StreamConfig) -> Result<(), String>
where
    T: cpal::Sample,
{
    let sample_rate = config.sample_rate.0 as f64;
    let channels = config.channels as usize;

    let midi_input = midir::MidiInput::new("Synth").unwrap();

    let device_port = find_port(&midi_input);
    if device_port.is_none() {
        println!("No input device found!");
        // return Err(String::from("No input device found!"));
    }

    let device_port = device_port.unwrap();

    let (sender, receiver) = channel::<MidiMessage>();

    let _connect_in = midi_input.connect(
        &device_port,
        DEVICE,
        move |_, data, sender| {
            let message = MidiMessage::from(data);
            sender.send(message);
        },
        sender,
    );

    // // Produce a sinusoid of maximum amplitude.
    // let oscillator = Arc::new(Mutex::new(Box::new(oscillator::Oscillator::new(
    //     config.clone(),
    //     oscillator::OscillatorShape::Triangle,
    //     130.81,
    // ))));

    // let oscillator1 = Arc::new(Mutex::new(Box::new(oscillator::Oscillator::new(
    //     config.clone(),
    //     oscillator::OscillatorShape::Sine,
    //     0.5,
    // ))));

    // let oscillator2 = Arc::new(Mutex::new(Box::new(oscillator::Oscillator::new(
    //     config.clone(),
    //     oscillator::OscillatorShape::Triangle,
    //     160.00,
    // ))));

    // // oscillator.lock().unwrap().use_freq_mod(oscillator1.clone());
    // // oscillator1.lock().unwrap().set_volume(50.0);

    // let stream = oscillator::Oscillator::output::<T>(oscillator.clone(), device);
    // stream.play().unwrap();

    // let stream1 = oscillator::Oscillator::output::<T>(oscillator2, device);
    // stream1.play().unwrap();

    // let osc = fundsp::oscillator::Sine::<f32>::new(config.sample_rate.0 as _);

    // loop {

    // }

    // let stream2 = oscillator::Oscillator::output::<T>(oscillator2, device);

    let synth = Arc::new(Mutex::new(synth::Synth::new(sample_rate)));
    let stream = synth::Synth::output::<T>(synth.clone(), device, config, receiver);
    stream.play().unwrap();

    loop {}

    Ok(())
}

// fn write_data<T, F>(output: &mut [T], channels: usize, mut sw: F)
// where
//     T: cpal::Sample,
//     F: FnMut() -> f32,
// {
//     for frame in output.chunks_mut(channels) {
//         // let p = &sw.lock().as_mut().unwrap().next_sample();
//         let value: T = cpal::Sample::from::<f32>(&mut sw());
//         for sample in frame.iter_mut() {
//             *sample = value;
//         }
//     }
// }

fn write_data<T>(output: &mut [T], channels: usize, next_sample: &mut dyn FnMut() -> (f64, f64))
where
    T: cpal::Sample,
{
    for frame in output.chunks_mut(channels) {
        let sample = next_sample();
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

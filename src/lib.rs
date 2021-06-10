#[macro_use]
extern crate vst;

use ahash::AHashMap;
use keyframe::{ease_with_scaled_time, functions::Linear};
use vst::{
    api::{Events, Supported},
    buffer::AudioBuffer,
    event::Event,
    plugin::{CanDo, Category, Info, Plugin},
};

/// Get the actual frequency represented by a u7 pitch and a u14 bend.
///
/// 8192 = 0 bend, 0 = -2 semitones, 16383 = +2 semitones.
fn midi_pitch_to_freq(pitch: u8, bend: u16) -> f64 {
    const A4_PITCH: i8 = 69; // nice
    const A4_FREQ: f64 = 440.0;

    let pitch = f64::from(pitch as i8 - A4_PITCH);
    // lerp bend from [0, 16383] to [-2.0, 2.0]
    let bend_amt = ease_with_scaled_time(Linear, -2.0, 2.0, bend as f64, 16383.0);

    // Midi notes can be 0-127
    ((pitch + bend_amt) / 12.).exp2() * A4_FREQ
}

struct Squarezoid {
    /// Map note pitches to their bend and velocity.
    notes: AHashMap<u8, Note>,
    /// Global pitch bend
    bend: u16,

    sample_rate: f64,
}

struct Note {
    velocity: u8,
    /// How long this note has been held for
    duration: f64,
}

impl Plugin for Squarezoid {
    fn get_info(&self) -> Info {
        Info {
            name: "Squarezoid".to_string(),
            vendor: "gamma-delta".to_string(),
            unique_id: 7979,
            category: Category::Synth,
            inputs: 0,
            outputs: 2,
            parameters: 0,
            initial_delay: 0,
            ..Default::default()
        }
    }

    fn process_events(&mut self, events: &Events) {
        for evt in events.events() {
            if let Event::Midi(evt) = evt {
                // https://www.midimountain.com/midi/midi_status.htm
                let data = evt.data;
                match data[0] {
                    // note off event
                    128..=143 => {
                        self.notes.remove(&data[1]);
                    }
                    // note on
                    144..=159 => {
                        self.notes.insert(
                            data[1],
                            Note {
                                duration: 0.0,
                                velocity: data[2],
                            },
                        );
                    }
                    // polyphonic aftertouch, aka they pressed harder or softer
                    // after pressing the button
                    160..=175 => {
                        if let Some(note) = self.notes.get_mut(&data[1]) {
                            note.velocity = data[2]
                        }
                    }
                    // pitch wheel
                    224..=239 => {
                        // the MSB are shifted over 7 because data[1] is a u7.
                        // it fills in the 8'th place, and then the next 6 bits
                        // to make a u14 in total.
                        self.bend = data[1] as u16 | ((data[2] as u16) << 7);
                    }
                    _ => {}
                }
            }
        }
    }

    fn set_sample_rate(&mut self, rate: f32) {
        self.sample_rate = rate as f64;
    }

    fn process(&mut self, buffer: &mut AudioBuffer<f32>) {
        let sample_count = buffer.samples();
        let (_, mut outputs) = buffer.split();

        let per_sample = self.sample_rate.recip();
        let output_count = outputs.len();

        for sample_idx in 0..sample_count {
            let bend = self.bend;
            let sample: f64 = self
                .notes
                .iter_mut()
                .map(|(&pitch, note)| {
                    let duty = note.velocity as f64 / 127.0;
                    let freq = midi_pitch_to_freq(pitch, bend);
                    let sample_time = note.duration * freq;

                    // how far are we along in this duty cycle?
                    let duty_progress = sample_time.fract();
                    let out = if duty_progress < duty { 0.0 } else { 0.01 };

                    note.duration += per_sample;
                    out
                })
                .sum();
            // go thru left and right channels
            for buf_idx in 0..output_count {
                let buf = outputs.get_mut(buf_idx);
                buf[sample_idx] = sample as f32;
            }
        }
    }

    fn can_do(&self, can_do: CanDo) -> Supported {
        match can_do {
            CanDo::ReceiveMidiEvent => Supported::Yes,
            _ => Supported::Maybe,
        }
    }
}
impl Default for Squarezoid {
    fn default() -> Self {
        Self {
            notes: AHashMap::with_capacity(8),
            sample_rate: 44100.0,
            bend: 8192,
        }
    }
}

plugin_main!(Squarezoid);

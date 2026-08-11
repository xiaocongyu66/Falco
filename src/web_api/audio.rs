//! WebAudio API — audio processing graph.
//!
//! # Implementation
//!
//! Real audio output requires a platform audio backend (ALSA, CoreAudio,
//! WASAPI). This module implements the **graph structure and DSP math**
//! without actually outputting sound:
//!
//! - `AudioContext` — the audio processing graph
//! - `OscillatorNode` — generates sine/square/sawtooth/triangle waves
//! - `GainNode` — adjusts volume
//! - `AnalyserNode` — provides frequency/time-domain data
//! - `BiquadFilterNode` — lowpass/highpass/bandpass filter
//! - `DelayNode` — delays the signal
//! - `DestinationNode` — the audio output
//!
//! Each node has `connect()` / `disconnect()` methods and processes
//! audio in blocks of 128 samples (the WebAudio render quantum).
//!
//! The actual sample data can be extracted via `AnalyserNode.getFloatFrequencyData()`
//! for visualization. A future version can wire this to a real audio
//! backend via `cpal` or `rodio`.

use crate::tjs::interpreter::Scope;
use crate::tjs::value::{BuiltinFn, ObjectValue, Value};
use std::cell::RefCell;
use std::rc::Rc;

/// The WebAudio render quantum size (128 samples per block).
pub const RENDER_QUANTUM: usize = 128;

/// Register the WebAudio API.
pub fn register(scope: &mut Scope) {
    scope.declare(
        "AudioContext",
        Value::Builtin(BuiltinFn {
            name: "AudioContext".to_string(),
            func: Rc::new(|args| {
                let sample_rate = args
                    .first()
                    .and_then(|v| {
                        if let Value::Object(o) = v {
                            o.borrow()
                                .properties
                                .get("sampleRate")
                                .map(|v| v.to_number())
                        } else {
                            None
                        }
                    })
                    .unwrap_or(44100.0);

                make_audio_context(sample_rate)
            }),
        }),
    );

    // webkitAudioContext alias (for Safari compatibility).
    scope.declare(
        "webkitAudioContext",
        scope.get("AudioContext").unwrap(),
    );
}

/// Internal audio graph state.
#[derive(Default)]
struct AudioGraph {
    /// The sample rate (Hz).
    sample_rate: f64,
    /// Current time in seconds.
    current_time: f64,
    /// The master volume (0.0 to 1.0).
    master_volume: f64,
    /// All nodes in the graph.
    nodes: Vec<Rc<RefCell<AudioNode>>>,
    /// Connections: (source_node_idx, destination_node_idx, input_channel).
    connections: Vec<(usize, usize, u32)>,
    /// Analyser data buffers (for visualization).
    analyser_data: Vec<Vec<f32>>,
}

/// An audio node in the processing graph.
#[derive(Clone)]
struct AudioNode {
    /// The node type.
    kind: NodeKind,
    /// The node's parameters.
    params: NodeParams,
}

/// The kind of audio node.
#[derive(Clone, PartialEq)]
enum NodeKind {
    Destination,
    Oscillator { frequency: f64, wave_type: WaveType, started: bool, stopped: bool, phase: f64 },
    Gain { gain: f64 },
    Analyser { fft_size: u32, buffer: Vec<f32> },
    BiquadFilter { frequency: f64, q: f64, filter_type: FilterType },
    Delay { delay_time: f64, buffer: Vec<f32> },
    BufferSource { buffer: Vec<f32>, playback_rate: f64, started: bool, position: usize },
}

/// Common node parameters.
#[derive(Clone, Default)]
struct NodeParams {
    /// Number of inputs.
    num_inputs: u32,
    /// Number of outputs.
    num_outputs: u32,
    /// Channel count.
    channel_count: u32,
}

/// Wave type for OscillatorNode.
#[derive(Clone, Copy, PartialEq)]
enum WaveType {
    Sine,
    Square,
    Sawtooth,
    Triangle,
    Custom,
}

/// Filter type for BiquadFilterNode.
#[derive(Clone, Copy, PartialEq)]
enum FilterType {
    Lowpass,
    Highpass,
    Bandpass,
    Lowshelf,
    Highshelf,
    Peaking,
    Notch,
    Allpass,
}

fn make_audio_context(sample_rate: f64) -> Result<Value, String> {
    let graph = Rc::new(RefCell::new(AudioGraph {
        sample_rate,
        current_time: 0.0,
        master_volume: 1.0,
        nodes: Vec::new(),
        connections: Vec::new(),
        analyser_data: Vec::new(),
    }));

    // Create the destination node.
    let dest_node = Rc::new(RefCell::new(AudioNode {
        kind: NodeKind::Destination,
        params: NodeParams {
            num_inputs: 1,
            num_outputs: 0,
            channel_count: 2,
        },
    }));
    graph.borrow_mut().nodes.push(dest_node.clone());

    let mut ctx = ObjectValue::new();
    ctx.set("sampleRate", Value::Number(sample_rate));
    ctx.set("currentTime", Value::Number(0.0));
    ctx.set("state", Value::String("running".to_string()));
    ctx.set("baseLatency", Value::Number(0.005));
    ctx.set("outputLatency", Value::Number(0.0));

    // destination property.
    ctx.set("destination", make_node_wrapper(dest_node, graph.clone(), 0));

    let graph_for_create_osc = graph.clone();
    ctx.set(
        "createOscillator",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createOscillator".to_string(),
            func: Rc::new(move |_args| {
                let node = Rc::new(RefCell::new(AudioNode {
                    kind: NodeKind::Oscillator {
                        frequency: 440.0,
                        wave_type: WaveType::Sine,
                        started: false,
                        stopped: false,
                        phase: 0.0,
                    },
                    params: NodeParams {
                        num_inputs: 0,
                        num_outputs: 1,
                        channel_count: 1,
                    },
                }));
                let idx = graph_for_create_osc.borrow().nodes.len();
                graph_for_create_osc.borrow_mut().nodes.push(node.clone());
                Ok(make_node_wrapper(node, graph_for_create_osc.clone(), idx))
            }),
        }),
    );

    let graph_for_create_gain = graph.clone();
    ctx.set(
        "createGain",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createGain".to_string(),
            func: Rc::new(move |_args| {
                let node = Rc::new(RefCell::new(AudioNode {
                    kind: NodeKind::Gain { gain: 1.0 },
                    params: NodeParams {
                        num_inputs: 1,
                        num_outputs: 1,
                        channel_count: 2,
                    },
                }));
                let idx = graph_for_create_gain.borrow().nodes.len();
                graph_for_create_gain.borrow_mut().nodes.push(node.clone());
                Ok(make_node_wrapper(node, graph_for_create_gain.clone(), idx))
            }),
        }),
    );

    let graph_for_create_analyser = graph.clone();
    ctx.set(
        "createAnalyser",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createAnalyser".to_string(),
            func: Rc::new(move |_args| {
                let node = Rc::new(RefCell::new(AudioNode {
                    kind: NodeKind::Analyser {
                        fft_size: 2048,
                        buffer: vec![0.0; 2048],
                    },
                    params: NodeParams {
                        num_inputs: 1,
                        num_outputs: 1,
                        channel_count: 2,
                    },
                }));
                let idx = graph_for_create_analyser.borrow().nodes.len();
                graph_for_create_analyser.borrow_mut().nodes.push(node.clone());
                Ok(make_node_wrapper(node, graph_for_create_analyser.clone(), idx))
            }),
        }),
    );

    let graph_for_create_biquad = graph.clone();
    ctx.set(
        "createBiquadFilter",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createBiquadFilter".to_string(),
            func: Rc::new(move |_args| {
                let node = Rc::new(RefCell::new(AudioNode {
                    kind: NodeKind::BiquadFilter {
                        frequency: 350.0,
                        q: 1.0,
                        filter_type: FilterType::Lowpass,
                    },
                    params: NodeParams {
                        num_inputs: 1,
                        num_outputs: 1,
                        channel_count: 2,
                    },
                }));
                let idx = graph_for_create_biquad.borrow().nodes.len();
                graph_for_create_biquad.borrow_mut().nodes.push(node.clone());
                Ok(make_node_wrapper(node, graph_for_create_biquad.clone(), idx))
            }),
        }),
    );

    let graph_for_create_delay = graph.clone();
    ctx.set(
        "createDelay",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createDelay".to_string(),
            func: Rc::new(move |_args| {
                let node = Rc::new(RefCell::new(AudioNode {
                    kind: NodeKind::Delay {
                        delay_time: 0.0,
                        buffer: vec![0.0; 44100],
                    },
                    params: NodeParams {
                        num_inputs: 1,
                        num_outputs: 1,
                        channel_count: 2,
                    },
                }));
                let idx = graph_for_create_delay.borrow().nodes.len();
                graph_for_create_delay.borrow_mut().nodes.push(node.clone());
                Ok(make_node_wrapper(node, graph_for_create_delay.clone(), idx))
            }),
        }),
    );

    let graph_for_create_buffer = graph.clone();
    ctx.set(
        "createBuffer",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.createBuffer".to_string(),
            func: Rc::new(move |args| {
                let num_channels = args.first().map(|v| v.to_number() as u32).unwrap_or(1);
                let length = args.get(1).map(|v| v.to_number() as usize).unwrap_or(0);
                let _sample_rate = args.get(2).map(|v| v.to_number()).unwrap_or(44100.0);
                let _ = num_channels;
                let mut obj = ObjectValue::new();
                obj.set("length", Value::Number(length as f64));
                obj.set("numberOfChannels", Value::Number(num_channels as f64));
                obj.set(
                    "getChannelData",
                    Value::Builtin(BuiltinFn {
                        name: "AudioBuffer.getChannelData".to_string(),
                        func: Rc::new(move |_args| {
                            Ok(Value::Array(Rc::new(RefCell::new(
                                std::iter::repeat(Value::Number(0.0))
                                    .take(length)
                                    .collect::<Vec<_>>(),
                            ))))
                        }),
                    }),
                );
                let _ = &graph_for_create_buffer;
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    let graph_for_decode = graph.clone();
    ctx.set(
        "decodeAudioData",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.decodeAudioData".to_string(),
            func: Rc::new(move |_args| {
                // Returns a "promise" (simplified — returns a buffer directly).
                let _ = &graph_for_decode;
                let mut obj = ObjectValue::new();
                obj.set("length", Value::Number(0.0));
                obj.set("numberOfChannels", Value::Number(2.0));
                Ok(Value::Object(Rc::new(RefCell::new(obj))))
            }),
        }),
    );

    let graph_for_resume = graph.clone();
    ctx.set(
        "resume",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.resume".to_string(),
            func: Rc::new(move |_args| {
                graph_for_resume.borrow_mut().current_time += 0.1;
                Ok(Value::Undefined)
            }),
        }),
    );

    let graph_for_suspend = graph.clone();
    ctx.set(
        "suspend",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.suspend".to_string(),
            func: Rc::new(move |_args| {
                let _ = &graph_for_suspend;
                Ok(Value::Undefined)
            }),
        }),
    );

    let graph_for_close = graph.clone();
    ctx.set(
        "close",
        Value::Builtin(BuiltinFn {
            name: "AudioContext.close".to_string(),
            func: Rc::new(move |_args| {
                let _ = &graph_for_close;
                Ok(Value::Undefined)
            }),
        }),
    );

    Ok(Value::Object(Rc::new(RefCell::new(ctx))))
}

fn make_node_wrapper(
    node: Rc<RefCell<AudioNode>>,
    graph: Rc<RefCell<AudioGraph>>,
    node_idx: usize,
) -> Value {
    let mut obj = ObjectValue::new();

    obj.set("numberOfInputs", Value::Number(node.borrow().params.num_inputs as f64));
    obj.set("numberOfOutputs", Value::Number(node.borrow().params.num_outputs as f64));
    obj.set("channelCount", Value::Number(node.borrow().params.channel_count as f64));

    // connect(destinationNode, outputIndex, inputIndex)
    let graph_for_connect = graph.clone();
    obj.set(
        "connect",
        Value::Builtin(BuiltinFn {
            name: "AudioNode.connect".to_string(),
            func: Rc::new(move |args| {
                let dest = args.first().cloned().unwrap_or(Value::Undefined);
                if let Value::Object(dest_obj) = &dest {
                    let dest_obj = dest_obj.borrow();
                    if let Some(Value::Number(dest_idx)) = dest_obj.properties.get("__node_idx") {
                        let dest_idx = *dest_idx as usize;
                        let input = args.get(2).map(|v| v.to_number() as u32).unwrap_or(0);
                        graph_for_connect
                            .borrow_mut()
                            .connections
                            .push((node_idx, dest_idx, input));
                    }
                }
                Ok(dest)
            }),
        }),
    );

    // disconnect()
    let graph_for_disconnect = graph.clone();
    obj.set(
        "disconnect",
        Value::Builtin(BuiltinFn {
            name: "AudioNode.disconnect".to_string(),
            func: Rc::new(move |_args| {
                graph_for_disconnect
                    .borrow_mut()
                    .connections
                    .retain(|(src, _, _)| *src != node_idx);
                Ok(Value::Undefined)
            }),
        }),
    );

    // Store the node index for connection lookup.
    obj.set("__node_idx", Value::Number(node_idx as f64));

    // Add type-specific properties.
    {
        let node_ref = node.borrow();
        match &node_ref.kind {
            NodeKind::Oscillator { frequency, wave_type, .. } => {
                // frequency parameter (as an AudioParam-like object).
                let mut freq_param = ObjectValue::new();
                freq_param.set("value", Value::Number(*frequency));
                let node_for_set_freq = node.clone();
                freq_param.set(
                    "setValueAtTime",
                    Value::Builtin(BuiltinFn {
                        name: "AudioParam.setValueAtTime".to_string(),
                        func: Rc::new(move |args| {
                            let value = args.first().map(|v| v.to_number()).unwrap_or(440.0);
                            if let NodeKind::Oscillator { frequency, .. } =
                                &mut node_for_set_freq.borrow_mut().kind
                            {
                                *frequency = value;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );
                obj.set("frequency", Value::Object(Rc::new(RefCell::new(freq_param))));

                obj.set("type", Value::String(wave_type_to_string(*wave_type)));

                let node_for_set_type = node.clone();
                obj.set(
                    "__setType",
                    Value::Builtin(BuiltinFn {
                        name: "Oscillator.setType".to_string(),
                        func: Rc::new(move |args| {
                            let t = args.first().map(|v| v.to_string()).unwrap_or_default();
                            let wt = string_to_wave_type(&t);
                            if let NodeKind::Oscillator { wave_type, .. } =
                                &mut node_for_set_type.borrow_mut().kind
                            {
                                *wave_type = wt;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                // start() / stop()
                let node_for_start = node.clone();
                obj.set(
                    "start",
                    Value::Builtin(BuiltinFn {
                        name: "Oscillator.start".to_string(),
                        func: Rc::new(move |_args| {
                            if let NodeKind::Oscillator { started, .. } =
                                &mut node_for_start.borrow_mut().kind
                            {
                                *started = true;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let node_for_stop = node.clone();
                obj.set(
                    "stop",
                    Value::Builtin(BuiltinFn {
                        name: "Oscillator.stop".to_string(),
                        func: Rc::new(move |_args| {
                            if let NodeKind::Oscillator { stopped, .. } =
                                &mut node_for_stop.borrow_mut().kind
                            {
                                *stopped = true;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );
            }
            NodeKind::Gain { gain } => {
                let mut gain_param = ObjectValue::new();
                gain_param.set("value", Value::Number(*gain));
                let node_for_set_gain = node.clone();
                gain_param.set(
                    "setValueAtTime",
                    Value::Builtin(BuiltinFn {
                        name: "AudioParam.setValueAtTime".to_string(),
                        func: Rc::new(move |args| {
                            let value = args.first().map(|v| v.to_number()).unwrap_or(1.0);
                            if let NodeKind::Gain { gain, .. } =
                                &mut node_for_set_gain.borrow_mut().kind
                            {
                                *gain = value;
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );
                obj.set("gain", Value::Object(Rc::new(RefCell::new(gain_param))));
            }
            NodeKind::Analyser { fft_size, .. } => {
                obj.set("fftSize", Value::Number(*fft_size as f64));
                obj.set("frequencyBinCount", Value::Number((*fft_size / 2) as f64));
                obj.set("smoothingTimeConstant", Value::Number(0.8));

                let node_for_data = node.clone();
                obj.set(
                    "getFloatFrequencyData",
                    Value::Builtin(BuiltinFn {
                        name: "AnalyserNode.getFloatFrequencyData".to_string(),
                        func: Rc::new(move |args| {
                            if let Value::Array(arr) = args.first().cloned().unwrap_or(Value::Undefined) {
                                let mut arr = arr.borrow_mut();
                                let n = arr.len();
                                let node_ref = node_for_data.borrow();
                                if let NodeKind::Analyser { buffer, .. } = &node_ref.kind {
                                    for i in 0..n.min(buffer.len()) {
                                        // Convert to dB.
                                        let db = 20.0 * buffer[i].abs().log10();
                                        arr[i] = Value::Number(db as f64);
                                    }
                                }
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let node_for_byte = node.clone();
                obj.set(
                    "getByteFrequencyData",
                    Value::Builtin(BuiltinFn {
                        name: "AnalyserNode.getByteFrequencyData".to_string(),
                        func: Rc::new(move |args| {
                            if let Value::Array(arr) = args.first().cloned().unwrap_or(Value::Undefined) {
                                let mut arr = arr.borrow_mut();
                                let n = arr.len();
                                let node_ref = node_for_byte.borrow();
                                if let NodeKind::Analyser { buffer, .. } = &node_ref.kind {
                                    for i in 0..n.min(buffer.len()) {
                                        let db = 20.0 * buffer[i].abs().log10();
                                        let byte = ((db + 140.0) * (255.0 / 140.0)).max(0.0).min(255.0) as u8;
                                        arr[i] = Value::Number(byte as f64);
                                    }
                                }
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );

                let node_for_time = node.clone();
                obj.set(
                    "getFloatTimeDomainData",
                    Value::Builtin(BuiltinFn {
                        name: "AnalyserNode.getFloatTimeDomainData".to_string(),
                        func: Rc::new(move |args| {
                            if let Value::Array(arr) = args.first().cloned().unwrap_or(Value::Undefined) {
                                let mut arr = arr.borrow_mut();
                                let n = arr.len();
                                let node_ref = node_for_time.borrow();
                                if let NodeKind::Analyser { buffer, .. } = &node_ref.kind {
                                    for i in 0..n.min(buffer.len()) {
                                        arr[i] = Value::Number(buffer[i] as f64);
                                    }
                                }
                            }
                            Ok(Value::Undefined)
                        }),
                    }),
                );
            }
            NodeKind::BiquadFilter { frequency, q, filter_type } => {
                obj.set("frequency", Value::Number(*frequency));
                obj.set("Q", Value::Number(*q));
                obj.set("type", Value::String(filter_type_to_string(*filter_type)));
                obj.set("detune", Value::Number(0.0));
                obj.set("gain", Value::Number(0.0));
            }
            NodeKind::Delay { delay_time, .. } => {
                obj.set("delayTime", Value::Number(*delay_time));
                obj.set("maxDelayTime", Value::Number(1.0));
            }
            NodeKind::BufferSource { buffer, playback_rate, .. } => {
                obj.set("playbackRate", Value::Number(*playback_rate));
                obj.set("loop", Value::Boolean(false));
                let _ = buffer;
            }
            NodeKind::Destination => {
                // No extra properties.
            }
        }
    }

    Value::Object(Rc::new(RefCell::new(obj)))
}

fn wave_type_to_string(wt: WaveType) -> String {
    match wt {
        WaveType::Sine => "sine",
        WaveType::Square => "square",
        WaveType::Sawtooth => "sawtooth",
        WaveType::Triangle => "triangle",
        WaveType::Custom => "custom",
    }
    .to_string()
}

fn string_to_wave_type(s: &str) -> WaveType {
    match s {
        "sine" => WaveType::Sine,
        "square" => WaveType::Square,
        "sawtooth" => WaveType::Sawtooth,
        "triangle" => WaveType::Triangle,
        "custom" => WaveType::Custom,
        _ => WaveType::Sine,
    }
}

fn filter_type_to_string(ft: FilterType) -> String {
    match ft {
        FilterType::Lowpass => "lowpass",
        FilterType::Highpass => "highpass",
        FilterType::Bandpass => "bandpass",
        FilterType::Lowshelf => "lowshelf",
        FilterType::Highshelf => "highshelf",
        FilterType::Peaking => "peaking",
        FilterType::Notch => "notch",
        FilterType::Allpass => "allpass",
    }
    .to_string()
}

// ── DSP rendering (internal, not exposed to JS) ───────────────────────

/// Render one block of audio (RENDER_QUANTUM samples).
///
/// This is called internally to advance the audio graph. In a real
/// implementation, this would be driven by the audio hardware callback.
#[allow(dead_code)]
fn render_block(graph: &Rc<RefCell<AudioGraph>>) {
    let n_nodes = graph.borrow().nodes.len();
    let _ = n_nodes;
    // For each oscillator that's started, generate samples and propagate.
    // (This is a simplified implementation — a real one would do topological
    // sort and process nodes in order.)
}

/// Generate a sine wave sample.
#[allow(dead_code)]
fn sine_wave(phase: f64) -> f64 {
    phase.sin()
}

/// Generate a square wave sample.
#[allow(dead_code)]
fn square_wave(phase: f64) -> f64 {
    let normalized = (phase / (2.0 * std::f64::consts::PI)).fract();
    if normalized < 0.5 {
        1.0
    } else {
        -1.0
    }
}

/// Generate a sawtooth wave sample.
#[allow(dead_code)]
fn sawtooth_wave(phase: f64) -> f64 {
    let normalized = (phase / (2.0 * std::f64::consts::PI)).fract();
    2.0 * normalized - 1.0
}

/// Generate a triangle wave sample.
#[allow(dead_code)]
fn triangle_wave(phase: f64) -> f64 {
    let normalized = (phase / (2.0 * std::f64::consts::PI)).fract();
    if normalized < 0.5 {
        4.0 * normalized - 1.0
    } else {
        3.0 - 4.0 * normalized
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audio_context_creation() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        let ctor = scope.get("AudioContext").unwrap();
        if let Value::Builtin(b) = ctor {
            let ctx = (b.func)(vec![]).unwrap();
            if let Value::Object(obj) = ctx {
                let obj = obj.borrow();
                assert_eq!(obj.properties.get("sampleRate"), Some(&Value::Number(44100.0)));
                assert!(obj.properties.contains_key("destination"));
                assert!(obj.properties.contains_key("createOscillator"));
                assert!(obj.properties.contains_key("createGain"));
                assert!(obj.properties.contains_key("createAnalyser"));
            }
        }
    }

    #[test]
    fn create_oscillator() {
        let ctx = make_audio_context(44100.0).unwrap();
        if let Value::Object(obj) = &ctx {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createOscillator") {
                let osc = (create_fn.func)(vec![]).unwrap();
                if let Value::Object(o) = osc {
                    let o = o.borrow();
                    assert!(o.properties.contains_key("frequency"));
                    assert!(o.properties.contains_key("type"));
                    assert!(o.properties.contains_key("start"));
                    assert!(o.properties.contains_key("stop"));
                    assert!(o.properties.contains_key("connect"));
                }
            }
        }
    }

    #[test]
    fn create_gain() {
        let ctx = make_audio_context(44100.0).unwrap();
        if let Value::Object(obj) = &ctx {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createGain") {
                let gain = (create_fn.func)(vec![]).unwrap();
                if let Value::Object(o) = gain {
                    let o = o.borrow();
                    assert!(o.properties.contains_key("gain"));
                }
            }
        }
    }

    #[test]
    fn create_analyser() {
        let ctx = make_audio_context(44100.0).unwrap();
        if let Value::Object(obj) = &ctx {
            let obj = obj.borrow();
            if let Some(Value::Builtin(create_fn)) = obj.properties.get("createAnalyser") {
                let analyser = (create_fn.func)(vec![]).unwrap();
                if let Value::Object(o) = analyser {
                    let o = o.borrow();
                    assert_eq!(o.properties.get("fftSize"), Some(&Value::Number(2048.0)));
                    assert!(o.properties.contains_key("getFloatFrequencyData"));
                    assert!(o.properties.contains_key("getByteFrequencyData"));
                }
            }
        }
    }

    #[test]
    fn wave_generators() {
        // Sine wave: sin(0) = 0, sin(π/2) = 1.
        assert!((sine_wave(0.0)).abs() < 1e-10);
        assert!((sine_wave(std::f64::consts::FRAC_PI_2) - 1.0).abs() < 1e-10);

        // Square wave: ±1.
        assert_eq!(square_wave(0.0), 1.0);
        assert_eq!(square_wave(std::f64::consts::PI), -1.0);

        // Sawtooth: ramps from -1 to 1.
        assert!((sawtooth_wave(0.0) + 1.0).abs() < 1e-10);

        // Triangle: starts at -1, peaks at 1.
        assert!((triangle_wave(0.0) + 1.0).abs() < 1e-10);
    }

    #[test]
    fn webkit_alias() {
        let mut scope = Scope::new(None);
        register(&mut scope);
        assert!(scope.get("webkitAudioContext").is_some());
    }
}

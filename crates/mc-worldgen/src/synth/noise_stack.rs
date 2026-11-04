//! `synth.NoiseStack`: a weighted sum of noises at different frequencies.
//!
//! Vanilla's `Builder.build` picks a `Perlin`/`SmearedPerlin` subclass when every layer is of that class;
//! those subclasses override `get`/`addToVolume` with bodies identical to the generic ones, so a single
//! type covers all three.

use std::sync::Arc;

use super::Noise;
use crate::densityfunction::sampler::{DensityBuffer, DensityVolume};
use crate::util::Interval;

/// `NoiseStack.Layer`.
#[derive(Clone)]
pub struct Layer {
    pub noise: Arc<dyn Noise>,
    pub frequency: f64,
    pub amplitude: f32,
}

pub struct NoiseStack {
    pub layers: Vec<Layer>,
    range: Interval,
}

impl NoiseStack {
    fn new(layers: Vec<Layer>) -> NoiseStack {
        let mut range = Interval::of_exact(0.0);
        for layer in &layers {
            range = Interval::add(
                range,
                Interval::mul(layer.noise.range(), Interval::of_exact(layer.amplitude)),
            );
        }
        NoiseStack { layers, range }
    }

    pub fn builder() -> NoiseStackBuilder {
        NoiseStackBuilder { layers: Vec::new() }
    }

    /// `getLayer`.
    pub fn get_layer(&self, index: usize) -> &Arc<dyn Noise> {
        &self.layers[index].noise
    }
}

impl Noise for NoiseStack {
    fn range(&self) -> Interval {
        self.range
    }

    fn get2(&self, x: f64, y: f64) -> f32 {
        let mut value = 0.0f32;
        for layer in &self.layers {
            let frequency = layer.frequency;
            value += layer.amplitude * layer.noise.get2(x * frequency, y * frequency);
        }
        value
    }

    fn get3(&self, x: f64, y: f64, z: f64) -> f32 {
        let mut value = 0.0f32;
        for layer in &self.layers {
            let frequency = layer.frequency;
            value += layer.amplitude
                * layer
                    .noise
                    .get3(x * frequency, y * frequency, z * frequency);
        }
        value
    }

    fn add_to_volume(
        &self,
        buffer: &mut DensityBuffer,
        volume: &DensityVolume,
        xz_scale: f64,
        y_scale: f64,
        amplitude: f32,
    ) {
        for layer in &self.layers {
            let frequency = layer.frequency;
            layer.noise.add_to_volume(
                buffer,
                volume,
                xz_scale * frequency,
                y_scale * frequency,
                amplitude * layer.amplitude,
            );
        }
    }
}

/// `NoiseStack.Builder`.
pub struct NoiseStackBuilder {
    layers: Vec<Layer>,
}

impl NoiseStackBuilder {
    pub fn add(mut self, noise: Arc<dyn Noise>, frequency: f64, amplitude: f32) -> Self {
        self.layers.push(Layer {
            noise,
            frequency,
            amplitude,
        });
        self
    }

    pub fn add_stack(mut self, stack: &NoiseStack, frequency: f64, amplitude: f32) -> Self {
        for layer in &stack.layers {
            self.layers.push(Layer {
                noise: layer.noise.clone(),
                frequency: layer.frequency * frequency,
                amplitude: layer.amplitude * amplitude,
            });
        }
        self
    }

    pub fn build(self) -> NoiseStack {
        NoiseStack::new(self.layers)
    }
}

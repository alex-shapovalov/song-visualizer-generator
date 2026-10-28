pub fn hash(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

pub fn random_unit(seed: u32) -> f32 {
    hash(seed) as f32 / u32::MAX as f32
}

pub fn lerp(from: f32, to: f32, amount: f32) -> f32 {
    from + (to - from) * amount
}

pub fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let x = ((value - low) / (high - low)).clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

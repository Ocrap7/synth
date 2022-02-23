use fundsp::Real;

pub fn midi_key_to_freq<R: Real>(key: u8) -> R {
    R::from_f64(440.0)
        * R::from_f64(2.0).pow((R::from_f64(key as _) - R::from_f64(69.0)) / R::from_f64(12.0))
}

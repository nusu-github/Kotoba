use std::collections::BTreeSet;

use crate::frontend::token::Particle;

pub fn particle_set(parts: &[Particle]) -> BTreeSet<&'static str> {
    parts.iter().map(Particle::as_str).collect()
}

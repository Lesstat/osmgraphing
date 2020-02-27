//------------------------------------------------------------------------------------------------//
// own modules

//------------------------------------------------------------------------------------------------//
// other modules

use super::{length::Meters, speed::KilometersPerHour, Metric, MetricU32};
use crate::network::NodeIdx;
use std::{
    fmt,
    fmt::Display,
    ops::Deref,
    ops::{Add, AddAssign, Index, IndexMut, Mul, MulAssign},
};

//------------------------------------------------------------------------------------------------//

#[derive(Debug, Default, Clone, Copy, Ord, PartialOrd, Eq, PartialEq)]
pub struct Milliseconds {
    value: u32,
}

impl Display for Milliseconds {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{} ms", self.value)
    }
}

impl Metric for Milliseconds {
    fn zero() -> Milliseconds {
        0u32.into()
    }

    fn neg_inf() -> Milliseconds {
        std::u32::MIN.into()
    }

    fn inf() -> Milliseconds {
        std::u32::MAX.into()
    }
}

impl From<u8> for Milliseconds {
    fn from(value: u8) -> Milliseconds {
        Milliseconds {
            value: value as u32,
        }
    }
}

impl From<u16> for Milliseconds {
    fn from(value: u16) -> Milliseconds {
        Milliseconds {
            value: value as u32,
        }
    }
}

impl From<u32> for Milliseconds {
    fn from(value: u32) -> Milliseconds {
        Milliseconds { value }
    }
}

impl From<MetricU32> for Milliseconds {
    fn from(metric: MetricU32) -> Milliseconds {
        (*metric).into()
    }
}

impl Into<MetricU32> for Milliseconds {
    fn into(self) -> MetricU32 {
        self.value.into()
    }
}

impl Deref for Milliseconds {
    type Target = u32;

    fn deref(&self) -> &u32 {
        &self.value
    }
}

//--------------------------------------------------------------------------------------------//
// indexing

impl Index<NodeIdx> for Vec<Milliseconds> {
    type Output = Milliseconds;

    fn index(&self, idx: NodeIdx) -> &Milliseconds {
        &self[*idx]
    }
}

impl IndexMut<NodeIdx> for Vec<Milliseconds> {
    fn index_mut(&mut self, idx: NodeIdx) -> &mut Milliseconds {
        &mut self[*idx]
    }
}

//--------------------------------------------------------------------------------------------//
// arithmetic operations

impl Add<Milliseconds> for Milliseconds {
    type Output = Milliseconds;

    fn add(self, other: Milliseconds) -> Milliseconds {
        Milliseconds {
            value: self.value + other.value,
        }
    }
}

impl AddAssign<Milliseconds> for Milliseconds {
    fn add_assign(&mut self, other: Milliseconds) {
        self.value += other.value;
    }
}

impl Mul<u32> for Milliseconds {
    type Output = Milliseconds;

    fn mul(self, scale: u32) -> Milliseconds {
        Milliseconds {
            value: scale * self.value,
        }
    }
}

impl MulAssign<u32> for Milliseconds {
    fn mul_assign(&mut self, scale: u32) {
        self.value *= scale;
    }
}

impl Mul<f64> for Milliseconds {
    type Output = Milliseconds;

    fn mul(self, scale: f64) -> Milliseconds {
        let new_value = scale * (self.value as f64) * scale;
        Milliseconds {
            value: new_value as u32,
        }
    }
}

impl MulAssign<f64> for Milliseconds {
    fn mul_assign(&mut self, scale: f64) {
        let new_value = scale * (self.value as f64);
        self.value = new_value as u32;
    }
}

/// s = v * t
impl Mul<KilometersPerHour> for Milliseconds {
    type Output = Meters;

    fn mul(self, rhs: KilometersPerHour) -> Meters {
        let time = self.value;
        let speed = *rhs;
        Meters::from(speed * time / 3_600)
    }
}

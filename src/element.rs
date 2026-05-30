use std::ops::Add; // disambiguate
use std::ops::*;

use bevy::math::U8Vec3;
use bevy::prelude::*;

const MASKS: U8Vec3 = U8Vec3::new(0x03, 0x0c, 0x30);
const SHIFTS: U8Vec3 = U8Vec3::new(0, 2, 4);

const fn itob(i: i32) -> u8 {
    match i {
        0 => 0,
        1 => 1,
        -1 => 2,
        _ => panic!("expected adjacent voxels"),
    }
}

fn btoi(b: u8) -> i32 {
    match b {
        0 => 0,
        1 => 1,
        2 => -1,
        _ => panic!("bad bit pattern"),
    }
}

/// Represents an element of the cube, ie a vertex, edge, face, or the interior.
/// Uses a u8, where bit 0 represents whether this element is on the +x face of
/// the cube, bit 1 -x, bit 2 +y, etc.
/// In some use cases the +x and -x bits may both be set, if something lies on both faces.
/// Eg for a 1 voxel thick wall, all of its voxels lie on both the right and left sides.
///
/// I'm thinking we should take the seventh bit to represent "inversion"... I'll wait until
/// we have more concrete use cases to define this more precisely.
///
/// parry3d's voxel collider uses a similar bit packing scheme. For them 0b00111111 represents
/// an isolated voxel, 0b00000000 represents an interior voxel, and 0b01000000
/// represents an empty voxel, which is consistent with our scheme.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Element(u8);

impl Element {
    /// No displacement.
    pub const ZERO: Self = Self::new(0, 0, 0);

    /// The positive X axis.
    pub const X: Self = Self::new(1, 0, 0);

    /// The positive Y axis.
    pub const Y: Self = Self::new(0, 1, 0);

    /// The positive Z axis.
    pub const Z: Self = Self::new(0, 0, 1);

    /// The negative X axis.
    pub const NEG_X: Self = Self::new(-1, 0, 0);

    /// The negative Y axis.
    pub const NEG_Y: Self = Self::new(0, -1, 0);

    /// The negative Z axis.
    pub const NEG_Z: Self = Self::new(0, 0, -1);

    /// The unit axes, in minor to major order
    pub const AXES: [Self; 3] = [Self::X, Self::Z, Self::Y];

    /// Displacements across a face, in minor to major order
    pub const FACES: [Self; 6] = [
        Self::X,
        Self::NEG_X,
        Self::Z,
        Self::NEG_Z,
        Self::Y,
        Self::NEG_Y,
    ];

    pub const FRAMES: [[Self; 3]; 3] = [
        [Self::X, Self::Y, Self::Z],
        [Self::Z, Self::X, Self::Y],
        [Self::Y, Self::Z, Self::X],
    ];

    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self((itob(x) << SHIFTS.x) | (itob(y) << SHIFTS.y) | (itob(z) << SHIFTS.z))
    }

    pub fn from_ivec3(v: IVec3) -> Self {
        Self::new(v.x, v.y, v.z)
    }

    pub fn as_ivec3(self) -> IVec3 {
        IVec3::new(
            btoi((self.0 & MASKS.x) >> SHIFTS.x),
            btoi((self.0 & MASKS.y) >> SHIFTS.y),
            btoi((self.0 & MASKS.z) >> SHIFTS.z),
        )
    }

    pub fn from_normal(n: Vec3) -> Self {
        let a = n.abs();
        if a.x >= a.y && a.x >= a.z {
            if n.x >= 0. { Self::X } else { Self::NEG_X }
        } else if a.y >= a.z {
            if n.y >= 0. { Self::Y } else { Self::NEG_Y }
        } else if n.z >= 0. {
            Self::Z
        } else {
            Self::NEG_Z
        }
    }
}

impl Add<Element> for IVec3 {
    type Output = IVec3;
    fn add(self, rhs: Element) -> Self::Output {
        self + rhs.as_ivec3()
    }
}

// impl Add<Element> for Element {
//     type Output = Element;
//     fn add(self, rhs: Element) -> Self::Output {
//         self + rhs.as_ivec3()
//     }
// }

impl Sub<Element> for IVec3 {
    type Output = IVec3;
    fn sub(self, rhs: Element) -> Self::Output {
        self - rhs.as_ivec3()
    }
}

impl Mul<i32> for Element {
    type Output = Element;
    fn mul(self, rhs: i32) -> Self::Output {
        match rhs {
            0 => Self::ZERO,
            1 => self,
            -1 => -self,
            _ => panic!("can only multiply an Element by 0, 1, -1"),
        }
    }
}

/// swaps opposite directions. preserves the two extra bits.
impl Neg for Element {
    type Output = Self;
    fn neg(self) -> Self::Output {
        Self(self.bitand(0xc0) | self.bitand(0x15).shl(1) | self.bitand(0x2a).shr(1))
    }
}

// should get this from deref right?
// impl BitOr for Element {
//     type Output = Element;
//     fn bitor(self, rhs: Element) -> Self::Output {
//         Element(self.0 | rhs.0)
//     }
// }

impl Deref for Element {
    type Target = u8;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

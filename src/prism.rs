use bevy::prelude::*;

pub struct Prism {
    min: IVec3,
    max: IVec3,
    current: IVec3,
}

pub fn prism(min: IVec3, max: IVec3) -> Prism {
    // maybe we should assert ordering instead of fixing it...
    let (min, max) = (min.min(max), max.max(min));
    Prism {
        min,
        max,
        current: if min.cmplt(max).all() { min } else { max },
    }
}

impl Iterator for Prism {
    type Item = IVec3;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current.y >= self.max.y {
            return None;
        }
        let current = self.current;
        self.current.x += 1;
        if self.current.x >= self.max.x {
            self.current.x = self.min.x;
            self.current.z += 1;
            if self.current.z >= self.max.z {
                self.current.z = self.min.z;
                self.current.y += 1;
            }
        }
        Some(current)
    }
}

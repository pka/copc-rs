#[derive(Clone, PartialEq, Debug)]
/// 3D bounding box
pub struct Bounds {
    pub min_x: f64,
    pub min_y: f64,
    pub min_z: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub max_z: f64,
}

impl Default for Bounds {
    fn default() -> Self {
        Bounds {
            min_x: f64::INFINITY,
            min_y: f64::INFINITY,
            min_z: f64::INFINITY,
            max_x: f64::NEG_INFINITY,
            max_y: f64::NEG_INFINITY,
            max_z: f64::NEG_INFINITY,
        }
    }
}

impl Bounds {
    pub fn new(min_x: f64, min_y: f64, min_z: f64, max_x: f64, max_y: f64, max_z: f64) -> Bounds {
        Bounds {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
        }
    }

    #[inline]
    pub fn reset(&mut self) {
        self.min_x = f64::INFINITY;
        self.min_y = f64::INFINITY;
        self.min_z = f64::INFINITY;
        self.max_x = f64::NEG_INFINITY;
        self.max_y = f64::NEG_INFINITY;
        self.max_z = f64::NEG_INFINITY;
    }

    pub fn sum(mut a: Bounds, b: &Bounds) -> Bounds {
        a.expand(b);
        a
    }

    #[inline]
    pub fn expand(&mut self, r: &Bounds) {
        if r.min_x < self.min_x {
            self.min_x = r.min_x;
        }
        if r.min_y < self.min_y {
            self.min_y = r.min_y;
        }
        if r.min_z < self.min_z {
            self.min_z = r.min_z;
        }
        if r.max_x > self.max_x {
            self.max_x = r.max_x;
        }
        if r.max_y > self.max_y {
            self.max_y = r.max_y;
        }
        if r.max_z > self.max_z {
            self.max_z = r.max_z;
        }
    }

    #[inline]
    pub fn expand_xyz(&mut self, x: f64, y: f64, z: f64) {
        if x < self.min_x {
            self.min_x = x;
        }
        if y < self.min_y {
            self.min_y = y;
        }
        if z < self.min_z {
            self.min_z = z;
        }
        if x > self.max_x {
            self.max_x = x;
        }
        if y > self.max_y {
            self.max_y = y;
        }
        if z > self.max_z {
            self.max_z = z;
        }
    }

    pub fn intersects(&self, r: &Bounds) -> bool {
        if self.max_x < r.min_x {
            return false;
        }
        if self.max_y < r.min_y {
            return false;
        }
        if self.max_z < r.min_z {
            return false;
        }
        if self.min_x > r.max_x {
            return false;
        }
        if self.min_y > r.max_y {
            return false;
        }
        if self.min_z > r.max_z {
            return false;
        }
        true
    }

    //     def ensure_3d(self, mins: np.ndarray, maxs: np.ndarray) -> "Bounds":
    //         new_mins = np.zeros(3, dtype=np.float64)
    //         new_maxs = np.zeros(3, dtype=np.float64)

    //         new_mins[: len(self.mins)] = self.mins[:]
    //         new_mins[len(self.mins) :] = mins[len(self.mins) :]
    //         new_maxs[: len(self.maxs)] = self.maxs[:]
    //         new_maxs[len(self.maxs) :] = maxs[len(self.maxs) :]

    //         return Bounds(new_mins, new_maxs)
}

/*
* Lattice geometry shared by the rule-table search engine.
*
* - Neighbourhood: ordered offsets whose set/unset state forms a cell's input pattern.
*   The position of an offset in the list is its bit in a pattern mask.
* - Lattice: packs a cell's coordinates into one i64, so "cell + offset" is one integer addition.
* - SymmetryGroup: signed coordinate permutations (the hyperoctahedral group) that map a
*   neighbourhood onto itself. A rule and its image under such a symmetry produce mirror-image
*   evolutions from a single seed, so they have identical cell counts at every step.
*/

use std::collections::{HashMap, HashSet};

/// Pattern masks are u64 and the engine keeps one spare bit as a "cell is set" flag.
pub const MAX_NEIGHBOURS: usize = 62;
pub const MAX_DIM: usize = 16;
/// The symmetry group is only enumerated up to this dimension (|B_5| = 3840).
pub const MAX_SYMMETRY_DIM: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Neighbourhood {
    pub dim: usize,
    pub offsets: Vec<Vec<i32>>,
}

impl Neighbourhood {
    pub fn new(dim: usize, offsets: Vec<Vec<i32>>) -> Result<Self, String> {
        if dim == 0 || dim > MAX_DIM {
            return Err(format!("dimension must be between 1 and {}", MAX_DIM));
        }
        if offsets.is_empty() {
            return Err("a neighbourhood needs at least one offset".to_string());
        }
        if offsets.len() > MAX_NEIGHBOURS {
            return Err(format!(
                "{} neighbours exceed the supported maximum of {}",
                offsets.len(),
                MAX_NEIGHBOURS
            ));
        }
        let mut seen: HashSet<&Vec<i32>> = HashSet::new();
        for offset in &offsets {
            if offset.len() != dim {
                return Err(format!("offset {:?} does not have {} coordinates", offset, dim));
            }
            if offset.iter().all(|&x| x == 0) {
                return Err("the zero offset is the cell itself, not a neighbour".to_string());
            }
            if !seen.insert(offset) {
                return Err(format!("duplicate offset {:?}", offset));
            }
        }
        Ok(Neighbourhood { dim, offsets })
    }

    /// All offsets within Chebyshev distance `radius`, excluding the cell itself.
    /// For radius 1 the order is the odometer order used by the legacy `Cell`
    /// (dimension 0 varies fastest, each digit goes 0, +1, -1).
    pub fn moore(dim: usize, radius: i32) -> Result<Self, String> {
        if radius < 1 {
            return Err("radius must be at least 1".to_string());
        }
        let side = (2 * radius + 1) as usize;
        let total = (side as u128).checked_pow(dim as u32).unwrap_or(u128::MAX);
        if total - 1 > MAX_NEIGHBOURS as u128 {
            return Err(format!(
                "Moore neighbourhood of radius {} in {} dimensions has {} neighbours; at most {} are supported",
                radius,
                dim,
                total - 1,
                MAX_NEIGHBOURS
            ));
        }
        let total = total as usize;
        let mut offsets = Vec::with_capacity(total - 1);
        for index in 1..total {
            let mut rest = index;
            let mut offset = Vec::with_capacity(dim);
            for _ in 0..dim {
                let digit = (rest % side) as i32;
                rest /= side;
                offset.push(match digit {
                    0 => 0,
                    d if d % 2 == 1 => (d + 1) / 2,
                    d => -(d / 2),
                });
            }
            offsets.push(offset);
        }
        Neighbourhood::new(dim, offsets)
    }

    /// The 2*dim axis neighbours: +e0, -e0, +e1, -e1, ...
    pub fn von_neumann(dim: usize) -> Result<Self, String> {
        let mut offsets = Vec::with_capacity(2 * dim);
        for axis in 0..dim {
            for sign in [1, -1] {
                let mut offset = vec![0; dim];
                offset[axis] = sign;
                offsets.push(offset);
            }
        }
        Neighbourhood::new(dim, offsets)
    }

    pub fn len(&self) -> usize {
        self.offsets.len()
    }

    /// Largest absolute coordinate of any offset.
    pub fn radius(&self) -> i32 {
        self.offsets
            .iter()
            .flat_map(|o| o.iter())
            .map(|x| x.abs())
            .max()
            .unwrap_or(0)
    }

    pub fn describe(&self) -> String {
        let parts: Vec<String> = self
            .offsets
            .iter()
            .map(|o| {
                let coords: Vec<String> = o.iter().map(|x| format!("{:+}", x)).collect();
                format!("({})", coords.join(","))
            })
            .collect();
        parts.join(" ")
    }

    /// The sub-neighbourhood made of the offsets at `indices`.
    pub fn select(&self, indices: &[usize]) -> Result<Self, String> {
        Neighbourhood::new(self.dim, indices.iter().map(|&i| self.offsets[i].clone()).collect())
    }
}

/// Mixed-radix packing of N coordinates into one i64 (power-of-two radix per coordinate).
#[derive(Debug, Clone)]
pub struct Lattice {
    dim: usize,
    bits: usize,
    digit_mask: i64,
    shift: i64,
    origin: i64,
}

impl Lattice {
    /// `extent` must bound |coordinate| of every cell the search can touch,
    /// including the neighbours of the outermost set cell.
    pub fn new(dim: usize, extent: i32) -> Result<Self, String> {
        let needed = extent as i64 + 1;
        let mut bits = 1usize;
        while (1i64 << (bits - 1)) <= needed {
            bits += 1;
        }
        if bits * dim > 62 {
            return Err(format!(
                "cells reaching distance {} in {} dimensions do not fit in 62 bits",
                extent, dim
            ));
        }
        let shift = 1i64 << (bits - 1);
        let mut lattice = Lattice {
            dim,
            bits,
            digit_mask: (1i64 << bits) - 1,
            shift,
            origin: 0,
        };
        lattice.origin = lattice.encode(&vec![0; dim]);
        Ok(lattice)
    }

    pub fn dim(&self) -> usize {
        self.dim
    }

    pub fn origin(&self) -> i64 {
        self.origin
    }

    pub fn encode(&self, coordinates: &[i32]) -> i64 {
        let mut cell = 0i64;
        for (i, &x) in coordinates.iter().enumerate() {
            cell |= (x as i64 + self.shift) << (self.bits * i);
        }
        cell
    }

    pub fn decode_into(&self, cell: i64, out: &mut [i32]) {
        for (i, slot) in out.iter_mut().enumerate().take(self.dim) {
            *slot = (((cell >> (self.bits * i)) & self.digit_mask) - self.shift) as i32;
        }
    }

    pub fn decode(&self, cell: i64) -> Vec<i32> {
        let mut out = vec![0; self.dim];
        self.decode_into(cell, &mut out);
        out
    }

    /// Encoded difference for an offset: `encode(x + offset) == encode(x) + delta(offset)`.
    pub fn delta(&self, offset: &[i32]) -> i64 {
        offset
            .iter()
            .enumerate()
            .map(|(i, &x)| (x as i64) * (1i64 << (self.bits * i)))
            .sum()
    }
}

/// A symmetry of the integer lattice: image coordinate i is `sign[i] * x[perm[i]]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupElement {
    pub perm: Vec<usize>,
    pub sign: Vec<i32>,
}

impl GroupElement {
    pub fn identity(dim: usize) -> Self {
        GroupElement {
            perm: (0..dim).collect(),
            sign: vec![1; dim],
        }
    }

    pub fn is_identity(&self) -> bool {
        self.sign.iter().all(|&s| s == 1) && self.perm.iter().enumerate().all(|(i, &p)| i == p)
    }

    pub fn apply_offset(&self, offset: &[i32]) -> Vec<i32> {
        (0..offset.len()).map(|i| self.sign[i] * offset[self.perm[i]]).collect()
    }
}

fn permutations(n: usize) -> Vec<Vec<usize>> {
    fn rec(prefix: &mut Vec<usize>, used: &mut Vec<bool>, n: usize, out: &mut Vec<Vec<usize>>) {
        if prefix.len() == n {
            out.push(prefix.clone());
            return;
        }
        for i in 0..n {
            if !used[i] {
                used[i] = true;
                prefix.push(i);
                rec(prefix, used, n, out);
                prefix.pop();
                used[i] = false;
            }
        }
    }
    let mut out = vec![];
    rec(&mut vec![], &mut vec![false; n], n, &mut out);
    out
}

/// Every signed permutation of the coordinate axes, identity included: 2^dim * dim! elements.
pub fn hyperoctahedral_group(dim: usize) -> Vec<GroupElement> {
    let mut elements = vec![];
    for perm in permutations(dim) {
        for signs in 0..(1usize << dim) {
            let sign = (0..dim).map(|i| if (signs >> i) & 1 == 1 { -1 } else { 1 }).collect();
            elements.push(GroupElement { perm: perm.clone(), sign });
        }
    }
    elements
}

/// The non-identity lattice symmetries that map a neighbourhood onto itself.
#[derive(Debug, Clone)]
pub struct SymmetryGroup {
    pub elements: Vec<GroupElement>,
}

impl SymmetryGroup {
    pub fn trivial() -> Self {
        SymmetryGroup { elements: vec![] }
    }

    /// Group order, counting the identity.
    pub fn order(&self) -> usize {
        self.elements.len() + 1
    }

    /// Dimensions above `MAX_SYMMETRY_DIM` get the trivial group (no symmetry reduction).
    pub fn preserving(neighbourhood: &Neighbourhood) -> Self {
        if neighbourhood.dim > MAX_SYMMETRY_DIM {
            return SymmetryGroup::trivial();
        }
        let offsets: HashSet<&Vec<i32>> = neighbourhood.offsets.iter().collect();
        let elements = hyperoctahedral_group(neighbourhood.dim)
            .into_iter()
            .filter(|g| !g.is_identity())
            .filter(|g| neighbourhood.offsets.iter().all(|o| offsets.contains(&g.apply_offset(o))))
            .collect();
        SymmetryGroup { elements }
    }

    pub fn apply_cell(&self, lattice: &Lattice, element: usize, cell: i64) -> i64 {
        let g = &self.elements[element];
        let dim = lattice.dim();
        let mut x = [0i32; MAX_DIM];
        let mut y = [0i32; MAX_DIM];
        lattice.decode_into(cell, &mut x[..dim]);
        for i in 0..dim {
            y[i] = g.sign[i] * x[g.perm[i]];
        }
        lattice.encode(&y[..dim])
    }
}

/// One representative per hyperoctahedral orbit of the `arity`-element subsets of the unit
/// Moore neighbourhood, as sorted index lists into `Neighbourhood::moore(dim, 1).offsets`.
/// Choosing the lexicographically smallest image makes the representative canonical.
pub fn subset_orbit_representatives(dim: usize, arity: usize) -> Result<Vec<Vec<usize>>, String> {
    if dim > MAX_SYMMETRY_DIM {
        return Err(format!("orbit enumeration supports dimension <= {}", MAX_SYMMETRY_DIM));
    }
    let moore = Neighbourhood::moore(dim, 1)?;
    let k = moore.len();
    if arity == 0 || arity > k {
        return Err(format!("arity must be between 1 and {}", k));
    }
    let index: HashMap<&Vec<i32>, usize> = moore.offsets.iter().enumerate().map(|(i, o)| (o, i)).collect();
    let images: Vec<Vec<usize>> = hyperoctahedral_group(dim)
        .iter()
        .filter(|g| !g.is_identity())
        .map(|g| moore.offsets.iter().map(|o| index[&g.apply_offset(o)]).collect())
        .collect();

    let mut representatives = vec![];
    let mut combo: Vec<usize> = (0..arity).collect();
    let mut image: Vec<usize> = Vec::with_capacity(arity);
    loop {
        let canonical = images.iter().all(|img| {
            image.clear();
            image.extend(combo.iter().map(|&j| img[j]));
            image.sort_unstable();
            image >= combo
        });
        if canonical {
            representatives.push(combo.clone());
        }

        let mut i = arity;
        loop {
            if i == 0 {
                return Ok(representatives);
            }
            i -= 1;
            if combo[i] != i + k - arity {
                break;
            }
        }
        combo[i] += 1;
        for j in i + 1..arity {
            combo[j] = combo[j - 1] + 1;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::cell::Cell;

    #[test]
    fn moore_order_matches_legacy_cell_odometer() {
        for dim in 1..=3 {
            let mut cell = Cell::new(dim);
            let mut legacy: Vec<Vec<i32>> = vec![];
            while cell.has_unexplored_nearby_cell() {
                cell.generate_next_unexplored_nearby_cell();
                legacy.push(cell.get_nearby_coordinate());
            }
            assert_eq!(Neighbourhood::moore(dim, 1).unwrap().offsets, legacy);
        }
    }

    #[test]
    fn neighbourhood_sizes_and_validation() {
        assert_eq!(Neighbourhood::moore(1, 1).unwrap().len(), 2);
        assert_eq!(Neighbourhood::moore(2, 1).unwrap().len(), 8);
        assert_eq!(Neighbourhood::moore(3, 1).unwrap().len(), 26);
        assert_eq!(Neighbourhood::moore(1, 4).unwrap().len(), 8);
        assert_eq!(Neighbourhood::moore(2, 2).unwrap().radius(), 2);
        assert_eq!(Neighbourhood::von_neumann(3).unwrap().len(), 6);
        assert!(Neighbourhood::moore(4, 1).is_err());
        assert!(Neighbourhood::new(2, vec![vec![0, 0]]).is_err());
        assert!(Neighbourhood::new(2, vec![vec![1, 0], vec![1, 0]]).is_err());
        assert!(Neighbourhood::new(2, vec![vec![1]]).is_err());
    }

    #[test]
    fn lattice_encoding_is_linear() {
        let lattice = Lattice::new(3, 20).unwrap();
        let x = [5, -7, 13];
        let offset = [1, -1, 1];
        let moved = [x[0] + offset[0], x[1] + offset[1], x[2] + offset[2]];
        assert_eq!(lattice.decode(lattice.encode(&x)), x.to_vec());
        assert_eq!(lattice.encode(&x) + lattice.delta(&offset), lattice.encode(&moved));
        assert_eq!(lattice.decode(lattice.origin()), vec![0, 0, 0]);
        assert!(Lattice::new(8, 1000).is_err());
    }

    #[test]
    fn symmetry_group_orders() {
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::moore(1, 1).unwrap()).order(), 2);
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::moore(2, 1).unwrap()).order(), 8);
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::moore(3, 1).unwrap()).order(), 48);
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::von_neumann(2).unwrap()).order(), 8);
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::von_neumann(3).unwrap()).order(), 48);
        assert_eq!(SymmetryGroup::preserving(&Neighbourhood::moore(1, 3).unwrap()).order(), 2);

        // Only the axis swap preserves {(1,0),(0,1),(1,1)}
        let causal = Neighbourhood::new(2, vec![vec![1, 0], vec![0, 1], vec![1, 1]]).unwrap();
        assert_eq!(SymmetryGroup::preserving(&causal).order(), 2);
        let asymmetric = Neighbourhood::new(2, vec![vec![1, 0], vec![0, 2], vec![1, 1]]).unwrap();
        assert_eq!(SymmetryGroup::preserving(&asymmetric).order(), 1);
    }

    #[test]
    fn group_action_on_cells_is_linear() {
        let nb = Neighbourhood::moore(3, 1).unwrap();
        let group = SymmetryGroup::preserving(&nb);
        let lattice = Lattice::new(3, 10).unwrap();
        let x = [3, -2, 5];
        for (index, g) in group.elements.iter().enumerate() {
            for offset in &nb.offsets {
                let moved = [x[0] + offset[0], x[1] + offset[1], x[2] + offset[2]];
                let image_of_moved = group.apply_cell(&lattice, index, lattice.encode(&moved));
                let image_of_x = group.apply_cell(&lattice, index, lattice.encode(&x));
                assert_eq!(image_of_moved, image_of_x + lattice.delta(&g.apply_offset(offset)));
            }
        }
    }

    fn binomial(n: usize, k: usize) -> usize {
        (0..k).fold(1usize, |acc, i| acc * (n - i) / (i + 1))
    }

    #[test]
    fn orbit_representatives_partition_all_subsets() {
        for (dim, arities) in [(2usize, 1..=8usize), (3, 1..=4)] {
            let moore = Neighbourhood::moore(dim, 1).unwrap();
            let k = moore.len();
            let group = hyperoctahedral_group(dim);
            let index: HashMap<&Vec<i32>, usize> =
                moore.offsets.iter().enumerate().map(|(i, o)| (o, i)).collect();
            for arity in arities {
                let reps = subset_orbit_representatives(dim, arity).unwrap();
                let mut covered = 0;
                for rep in &reps {
                    let mut orbit: HashSet<Vec<usize>> = HashSet::new();
                    for g in &group {
                        let mut image: Vec<usize> =
                            rep.iter().map(|&j| index[&g.apply_offset(&moore.offsets[j])]).collect();
                        image.sort_unstable();
                        orbit.insert(image);
                    }
                    covered += orbit.len();
                }
                assert_eq!(covered, binomial(k, arity), "dim {} arity {}", dim, arity);
            }
        }
    }

    #[test]
    fn orbit_counts_in_two_dimensions() {
        let counts: Vec<usize> = (1..=8)
            .map(|r| subset_orbit_representatives(2, r).unwrap().len())
            .collect();
        // 50 non-empty subsets of the 8 neighbours up to the symmetries of the square (Burnside)
        assert_eq!(counts.iter().sum::<usize>(), 50);
        assert_eq!(counts[0], 2);
        assert_eq!(counts[1], 6);
        assert_eq!(counts[7], 1);
        assert_eq!(subset_orbit_representatives(3, 1).unwrap().len(), 3);
    }
}

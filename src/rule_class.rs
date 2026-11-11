/*
* Rule classes: which neighbourhood patterns share one table entry.
*
*  table       every pattern (bit mask of set neighbours) is its own entry: 2^k - 1 birth entries
*              (the empty pattern never births) and 2^k survival entries.
*  isotropic   patterns related by a symmetry of the neighbourhood share an entry, so the rule looks
*              the same after every rotation and reflection. For the 2D Moore neighbourhood these are
*              Hensel's isotropic rules: 50 birth and 51 survival entries.
*  totalistic  only the number of set neighbours matters; the cell's own state still selects birth
*              or survival ("outer totalistic", e.g. the Life-like rules B3/S23: k birth and k + 1
*              survival entries).
*
* Each pattern maps to a key (the smallest mask of its symmetry class, or its neighbour count) and
* the search assigns one value per key, so a rule of a smaller class is a table with tied entries.
*/

use crate::lattice::{apply_matrix, Neighbourhood, SymmetryGroup};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleClass {
    Table,
    Isotropic,
    Totalistic,
}

impl RuleClass {
    pub fn parse(text: &str) -> Result<Self, String> {
        match text {
            "table" => Ok(RuleClass::Table),
            "isotropic" => Ok(RuleClass::Isotropic),
            "totalistic" => Ok(RuleClass::Totalistic),
            other => Err(format!(
                "unknown rule class '{}' (table | isotropic | totalistic)",
                other
            )),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RuleClass::Table => "table",
            RuleClass::Isotropic => "isotropic",
            RuleClass::Totalistic => "totalistic",
        }
    }
}

/// Isotropic keys are precomputed for neighbourhoods up to this size.
const DENSE_KEYS: usize = 16;

#[derive(Clone, Debug)]
pub struct Classifier {
    class: RuleClass,
    /// The non-identity symmetries of the neighbourhood as permutations of neighbour indices.
    perms: Vec<Vec<u8>>,
    dense: Vec<u64>,
    /// Number of keys over all 2^k patterns, the empty pattern included.
    pattern_classes: u128,
}

impl Classifier {
    pub fn new(class: RuleClass, nb: &Neighbourhood) -> Result<Self, String> {
        Classifier::with_symmetries(class, nb, &[])
    }

    /// As `new`, with the isotropy group given as integer matrices (for lattices such as the
    /// hexagonal one whose symmetries are not signed permutations). Empty: the signed permutations
    /// that preserve the neighbourhood.
    pub fn with_symmetries(
        class: RuleClass,
        nb: &Neighbourhood,
        matrices: &[Vec<Vec<i32>>],
    ) -> Result<Self, String> {
        let k = nb.len();
        let mut perms = vec![];
        if class == RuleClass::Isotropic {
            let maps: Vec<Box<dyn Fn(&[i32]) -> Vec<i32>>> = if matrices.is_empty() {
                SymmetryGroup::preserving(nb)
                    .elements
                    .into_iter()
                    .map(|g| {
                        Box::new(move |o: &[i32]| g.apply_offset(o))
                            as Box<dyn Fn(&[i32]) -> Vec<i32>>
                    })
                    .collect()
            } else {
                matrices
                    .iter()
                    .filter(|m| {
                        (0..nb.dim).any(|i| (0..nb.dim).any(|j| m[i][j] != (i == j) as i32))
                    })
                    .map(|m| {
                        let m = m.clone();
                        Box::new(move |o: &[i32]| apply_matrix(&m, o))
                            as Box<dyn Fn(&[i32]) -> Vec<i32>>
                    })
                    .collect()
            };
            for g in &maps {
                let perm = nb
                    .offsets
                    .iter()
                    .map(|offset| {
                        let image = g(offset);
                        nb.offsets
                            .iter()
                            .position(|other| *other == image)
                            .map(|index| index as u8)
                            .ok_or_else(|| "a symmetry left the neighbourhood".to_string())
                    })
                    .collect::<Result<Vec<u8>, String>>()?;
                perms.push(perm);
            }
        }
        let pattern_classes = match class {
            RuleClass::Table => 1u128 << k,
            RuleClass::Totalistic => k as u128 + 1,
            RuleClass::Isotropic => burnside(k, &perms),
        };
        let mut classifier = Classifier {
            class,
            perms,
            dense: vec![],
            pattern_classes,
        };
        if class == RuleClass::Isotropic && k <= DENSE_KEYS {
            classifier.dense = (0..1u64 << k)
                .map(|mask| classifier.canonical(mask))
                .collect();
        }
        Ok(classifier)
    }

    pub fn class(&self) -> RuleClass {
        self.class
    }

    #[inline]
    pub fn key(&self, mask: u64) -> u64 {
        match self.class {
            RuleClass::Table => mask,
            RuleClass::Totalistic => mask.count_ones() as u64,
            RuleClass::Isotropic if !self.dense.is_empty() => self.dense[mask as usize],
            RuleClass::Isotropic => self.canonical(mask),
        }
    }

    fn canonical(&self, mask: u64) -> u64 {
        let mut best = mask;
        for perm in &self.perms {
            let mut image = 0u64;
            let mut rest = mask;
            while rest != 0 {
                let j = rest.trailing_zeros() as usize;
                rest &= rest - 1;
                image |= 1u64 << perm[j];
            }
            best = best.min(image);
        }
        best
    }

    /// Birth entries of the class (the empty pattern never births: space stays empty).
    pub fn birth_entries(&self) -> u128 {
        self.pattern_classes - 1
    }

    pub fn survival_entries(&self) -> u128 {
        self.pattern_classes
    }

    /// Symmetries the rule is invariant under, the identity included.
    pub fn invariance_order(&self) -> usize {
        self.perms.len() + 1
    }
}

/// Number of orbits of the 2^k patterns: the average over the group of 2^(cycles of the element).
fn burnside(k: usize, perms: &[Vec<u8>]) -> u128 {
    let mut total = 1u128 << k;
    for perm in perms {
        let mut seen = vec![false; perm.len()];
        let mut cycles = 0;
        for start in 0..perm.len() {
            if !seen[start] {
                cycles += 1;
                let mut j = start;
                while !seen[j] {
                    seen[j] = true;
                    j = perm[j] as usize;
                }
            }
        }
        total += 1u128 << cycles;
    }
    total / (perms.len() as u128 + 1)
}

/// Life-like notation of the consulted entries of a totalistic rule, e.g. "B3/S23".
/// Unconsulted counts are left out; `?` lists them.
pub fn totalistic_notation(
    k: usize,
    birth: &[(u64, bool)],
    survival: Option<&[(u64, bool)]>,
) -> String {
    let on = |entries: &[(u64, bool)]| -> String {
        let mut counts: Vec<u64> = entries
            .iter()
            .filter(|(_, v)| *v)
            .map(|(c, _)| *c)
            .collect();
        counts.sort_unstable();
        counts
            .iter()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    let free = |entries: &[(u64, bool)], from: u64| -> String {
        let consulted: Vec<u64> = entries.iter().map(|(c, _)| *c).collect();
        let open: Vec<String> = (from..=k as u64)
            .filter(|c| !consulted.contains(c))
            .map(|c| c.to_string())
            .collect();
        if open.is_empty() {
            String::new()
        } else {
            format!(" (free: {})", open.join(","))
        }
    };
    let mut text = format!("B{}{}", on(birth), free(birth, 1));
    if let Some(survival) = survival {
        text.push_str(&format!(" / S{}{}", on(survival), free(survival, 0)));
    }
    text
}

#[cfg(test)]
mod test {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn class_sizes() {
        let moore = Neighbourhood::moore(2, 1).unwrap();
        let iso = Classifier::new(RuleClass::Isotropic, &moore).unwrap();
        assert_eq!((iso.birth_entries(), iso.survival_entries()), (50, 51));
        assert_eq!(iso.invariance_order(), 8);
        let tot = Classifier::new(RuleClass::Totalistic, &moore).unwrap();
        assert_eq!((tot.birth_entries(), tot.survival_entries()), (8, 9));
        let table = Classifier::new(RuleClass::Table, &moore).unwrap();
        assert_eq!(
            (table.birth_entries(), table.survival_entries()),
            (255, 256)
        );

        let vn = Neighbourhood::von_neumann(2).unwrap();
        assert_eq!(
            Classifier::new(RuleClass::Isotropic, &vn)
                .unwrap()
                .survival_entries(),
            6
        );
        let cube = Neighbourhood::moore(3, 1).unwrap();
        let iso3 = Classifier::new(RuleClass::Isotropic, &cube).unwrap();
        assert_eq!(iso3.invariance_order(), 48);
        // Orbits of the 2^26 neighbour patterns of the cube under its 48 symmetries.
        assert_eq!(iso3.survival_entries(), 1_426_144);
    }

    #[test]
    fn isotropic_keys_match_a_naive_coordinate_canonicalisation() {
        // Independent check: transform the set neighbour offsets with the 8 symmetries of the square.
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let classifier = Classifier::new(RuleClass::Isotropic, &nb).unwrap();
        let maps: [fn(i32, i32) -> (i32, i32); 8] = [
            |x, y| (x, y),
            |x, y| (-y, x),
            |x, y| (-x, -y),
            |x, y| (y, -x),
            |x, y| (-x, y),
            |x, y| (x, -y),
            |x, y| (y, x),
            |x, y| (-y, -x),
        ];
        let index: HashMap<(i32, i32), usize> = nb
            .offsets
            .iter()
            .enumerate()
            .map(|(j, o)| ((o[0], o[1]), j))
            .collect();
        let mut naive_classes: HashMap<u64, u64> = HashMap::new();
        for mask in 0..256u64 {
            let naive = maps
                .iter()
                .map(|f| {
                    let mut image = 0u64;
                    for (j, o) in nb.offsets.iter().enumerate() {
                        if mask >> j & 1 == 1 {
                            image |= 1 << index[&f(o[0], o[1])];
                        }
                    }
                    image
                })
                .min()
                .unwrap();
            assert_eq!(classifier.key(mask), naive, "mask {:08b}", mask);
            naive_classes.insert(naive, mask);
        }
        assert_eq!(naive_classes.len(), 51);
    }

    #[test]
    fn hexagonal_isotropy_is_the_bracelet_classification() {
        // The six hexagonal neighbours form a ring, and the 12 lattice symmetries act on it as the
        // rotations and reflections of a bracelet: 13 bracelets of 6 two-coloured beads.
        let hex = Neighbourhood::hexagonal();
        let c = Classifier::with_symmetries(
            RuleClass::Isotropic,
            &hex,
            &crate::lattice::hexagonal_symmetries(),
        )
        .unwrap();
        assert_eq!(
            (
                c.birth_entries(),
                c.survival_entries(),
                c.invariance_order()
            ),
            (12, 13, 12)
        );
        let ring = |mask: u64, shift: u32, flip: bool| -> u64 {
            (0..6)
                .filter(|&j| mask >> j & 1 == 1)
                .fold(0u64, |image, j| {
                    let position = if flip { (6 - j) % 6 } else { j };
                    image | 1 << ((position + shift) % 6)
                })
        };
        for mask in 0..64u64 {
            let naive = (0..6)
                .flat_map(|s| [ring(mask, s, false), ring(mask, s, true)])
                .min()
                .unwrap();
            assert_eq!(c.key(mask), naive, "mask {:06b}", mask);
        }
        // Without the matrices only the square symmetries that preserve the hexagon are found.
        let square = Classifier::new(RuleClass::Isotropic, &hex).unwrap();
        assert_eq!(square.invariance_order(), 4);
    }

    #[test]
    fn totalistic_keys_and_notation() {
        let nb = Neighbourhood::moore(2, 1).unwrap();
        let c = Classifier::new(RuleClass::Totalistic, &nb).unwrap();
        assert_eq!(c.key(0b1011_0001), 4);
        let birth = [(3u64, true), (1, false)];
        let survival = [(2u64, true), (3, true), (4, false)];
        assert_eq!(
            totalistic_notation(8, &birth, Some(&survival)),
            "B3 (free: 2,4,5,6,7,8) / S2,3 (free: 0,1,5,6,7,8)"
        );
    }
}

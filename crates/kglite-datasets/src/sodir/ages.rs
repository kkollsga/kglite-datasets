//! Fixed age normalization for the public Sodir wellbore and play labels.
//!
//! This is deliberately the vocabulary used by the NJU-1 discovery/play
//! workflow, not a general stratigraphy model. Source strings remain on their
//! nodes; these ordered atoms are only used to compare those strings.

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AgeAtom {
    PreDevonian,
    Devonian,
    Carboniferous,
    Permian,
    LowerTriassic,
    MiddleTriassic,
    UpperTriassic,
    LowerJurassic,
    MiddleJurassic,
    UpperJurassic,
    LowerCretaceous,
    UpperCretaceous,
    Paleocene,
    Eocene,
    Oligocene,
    Miocene,
    Pliocene,
    Pleistocene,
    Holocene,
}

impl AgeAtom {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreDevonian => "Pre-Devonian",
            Self::Devonian => "Devonian",
            Self::Carboniferous => "Carboniferous",
            Self::Permian => "Permian",
            Self::LowerTriassic => "Lower Triassic",
            Self::MiddleTriassic => "Middle Triassic",
            Self::UpperTriassic => "Upper Triassic",
            Self::LowerJurassic => "Lower Jurassic",
            Self::MiddleJurassic => "Middle Jurassic",
            Self::UpperJurassic => "Upper Jurassic",
            Self::LowerCretaceous => "Lower Cretaceous",
            Self::UpperCretaceous => "Upper Cretaceous",
            Self::Paleocene => "Paleocene",
            Self::Eocene => "Eocene",
            Self::Oligocene => "Oligocene",
            Self::Miocene => "Miocene",
            Self::Pliocene => "Pliocene",
            Self::Pleistocene => "Pleistocene",
            Self::Holocene => "Holocene",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Compatibility {
    Partial,
    Full,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgeMatch {
    pub compatibility: Compatibility,
    pub atoms: Vec<AgeAtom>,
}

use AgeAtom::*;

const PRE_DEVONIAN: &[AgeAtom] = &[PreDevonian];
const DEVONIAN: &[AgeAtom] = &[Devonian];
const CARBONIFEROUS: &[AgeAtom] = &[Carboniferous];
const PERMIAN: &[AgeAtom] = &[Permian];
const LOWER_TRIASSIC: &[AgeAtom] = &[LowerTriassic];
const MIDDLE_TRIASSIC: &[AgeAtom] = &[MiddleTriassic];
const UPPER_TRIASSIC: &[AgeAtom] = &[UpperTriassic];
const TRIASSIC: &[AgeAtom] = &[LowerTriassic, MiddleTriassic, UpperTriassic];
const LOWER_JURASSIC: &[AgeAtom] = &[LowerJurassic];
const MIDDLE_JURASSIC: &[AgeAtom] = &[MiddleJurassic];
const UPPER_JURASSIC: &[AgeAtom] = &[UpperJurassic];
const JURASSIC: &[AgeAtom] = &[LowerJurassic, MiddleJurassic, UpperJurassic];
const TRIASSIC_JURASSIC: &[AgeAtom] = &[
    LowerTriassic,
    MiddleTriassic,
    UpperTriassic,
    LowerJurassic,
    MiddleJurassic,
    UpperJurassic,
];
const LOWER_MIDDLE_JURASSIC: &[AgeAtom] = &[LowerJurassic, MiddleJurassic];
const MIDDLE_UPPER_JURASSIC: &[AgeAtom] = &[MiddleJurassic, UpperJurassic];
const TRIASSIC_LOWER_MIDDLE_JURASSIC: &[AgeAtom] = &[
    LowerTriassic,
    MiddleTriassic,
    UpperTriassic,
    LowerJurassic,
    MiddleJurassic,
];
const LOWER_CRETACEOUS: &[AgeAtom] = &[LowerCretaceous];
const UPPER_CRETACEOUS: &[AgeAtom] = &[UpperCretaceous];
const CRETACEOUS: &[AgeAtom] = &[LowerCretaceous, UpperCretaceous];
const PALEOCENE: &[AgeAtom] = &[Paleocene];
const EOCENE: &[AgeAtom] = &[Eocene];
const OLIGOCENE: &[AgeAtom] = &[Oligocene];
const MIOCENE: &[AgeAtom] = &[Miocene];
const PLIOCENE: &[AgeAtom] = &[Pliocene];
const PLEISTOCENE: &[AgeAtom] = &[Pleistocene];
const TERTIARY: &[AgeAtom] = &[Paleocene, Eocene, Oligocene, Miocene, Pliocene];
const PRE_TRIASSIC: &[AgeAtom] = &[PreDevonian, Devonian, Carboniferous, Permian];
const PRE_JURASSIC: &[AgeAtom] = &[
    PreDevonian,
    Devonian,
    Carboniferous,
    Permian,
    LowerTriassic,
    MiddleTriassic,
    UpperTriassic,
];
const POST_PALEOCENE: &[AgeAtom] = &[Eocene, Oligocene, Miocene, Pliocene, Pleistocene, Holocene];
const CARBONIFEROUS_PERMIAN: &[AgeAtom] = &[Carboniferous, Permian];
const PERMIAN_PALEOCENE: &[AgeAtom] = &[
    Permian,
    LowerTriassic,
    MiddleTriassic,
    UpperTriassic,
    LowerJurassic,
    MiddleJurassic,
    UpperJurassic,
    LowerCretaceous,
    UpperCretaceous,
    Paleocene,
];

/// Normalize one literal public Sodir age value to ordered canonical atoms.
pub fn normalize(source: &str) -> Option<&'static [AgeAtom]> {
    match source.trim() {
        "PRE-DEVONIAN" => Some(PRE_DEVONIAN),
        "DEVONIAN" => Some(DEVONIAN),
        "CARBONIFEROUS" | "LATE CARBONIFEROUS" => Some(CARBONIFEROUS),
        "PERMIAN" | "EARLY PERMIAN" | "LATE PERMIAN" => Some(PERMIAN),
        "EARLY TRIASSIC" => Some(LOWER_TRIASSIC),
        "MIDDLE TRIASSIC" => Some(MIDDLE_TRIASSIC),
        "LATE TRIASSIC" | "RHAETIAN" => Some(UPPER_TRIASSIC),
        "TRIAS" | "TRIASSIC" => Some(TRIASSIC),
        "EARLY JURASSIC" => Some(LOWER_JURASSIC),
        "MIDDLE JURASSIC" | "CALLOVIAN" => Some(MIDDLE_JURASSIC),
        "LATE JURASSIC" | "OXFORDIAN" => Some(UPPER_JURASSIC),
        "EARLY/MID JURASSIC" => Some(LOWER_MIDDLE_JURASSIC),
        "JURASSIC" => Some(JURASSIC),
        "JURASSIC/TRIASSIC" => Some(TRIASSIC_JURASSIC),
        "EARLY CRETACEOUS" => Some(LOWER_CRETACEOUS),
        "LATE CRETACEOUS" | "CAMPANIAN" => Some(UPPER_CRETACEOUS),
        "CRETACEOUS" => Some(CRETACEOUS),
        "DANIAN" | "EARLY PALEOCENE" | "LATE PALEOCENE" | "PALEOCENE" => Some(PALEOCENE),
        "EARLY EOCENE" | "EOCENE" => Some(EOCENE),
        "OLIGOCENE" => Some(OLIGOCENE),
        "EARLY MIOCENE" | "MIOCENE" => Some(MIOCENE),
        "PLIOCENE" => Some(PLIOCENE),
        "PLEISTOCENE" => Some(PLEISTOCENE),
        "TERTIARY" => Some(TERTIARY),
        "Carboniferous" => Some(CARBONIFEROUS),
        "Carboniferous-Permian" => Some(CARBONIFEROUS_PERMIAN),
        "Cretaceous" => Some(CRETACEOUS),
        "Eocene" => Some(EOCENE),
        "Lower Cretaceous" => Some(LOWER_CRETACEOUS),
        "Lower-Middle Jurassic" => Some(LOWER_MIDDLE_JURASSIC),
        "Middle Jurassic" => Some(MIDDLE_JURASSIC),
        "Middle-Upper Jurassic" => Some(MIDDLE_UPPER_JURASSIC),
        "Miocen" => Some(MIOCENE),
        "Paleocene" => Some(PALEOCENE),
        "Permian" => Some(PERMIAN),
        "Permian-Palaeocene" => Some(PERMIAN_PALEOCENE),
        "Pleistocene" => Some(PLEISTOCENE),
        "Post Palaeocene" => Some(POST_PALEOCENE),
        "Pre-Jurassic" => Some(PRE_JURASSIC),
        "Pre-Triassic" => Some(PRE_TRIASSIC),
        "Triassic, Lower" => Some(LOWER_TRIASSIC),
        "Triassic, Lower-Middle" => Some(&[LowerTriassic, MiddleTriassic]),
        // The literal source value crosses the period boundary. Keep all
        // Triassic atoms plus Lower–Middle Jurassic; plyGroupName is narrower.
        "Triassic, Lower-Middle Jurassic" => Some(TRIASSIC_LOWER_MIDDLE_JURASSIC),
        "Triassic, Upper" => Some(UPPER_TRIASSIC),
        "Upper Cretaceous" => Some(UPPER_CRETACEOUS),
        "Upper Jurassic" => Some(UPPER_JURASSIC),
        // Basement has no safe equivalence to PRE-DEVONIAN in this workflow.
        "" | "INDETERMINATE" | "Basement" => None,
        _ => None,
    }
}

/// Compare one well hydrocarbon-age slot with one play age.
///
/// Full means every atom in the well slot is covered by the play. Partial
/// means the two known sets overlap. Unknown values carry no evidence.
pub fn compatibility(well_age: &str, play_age: &str) -> Option<AgeMatch> {
    let well = normalize(well_age)?;
    let play = normalize(play_age)?;
    let atoms: Vec<AgeAtom> = well
        .iter()
        .copied()
        .filter(|atom| play.contains(atom))
        .collect();
    if atoms.is_empty() {
        return None;
    }
    Some(AgeMatch {
        compatibility: if atoms.len() == well.len() {
            Compatibility::Full
        } else {
            Compatibility::Partial
        },
        atoms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WELL_LABELS: &[&str] = &[
        "CALLOVIAN",
        "CAMPANIAN",
        "CARBONIFEROUS",
        "CRETACEOUS",
        "DANIAN",
        "DEVONIAN",
        "EARLY CRETACEOUS",
        "EARLY EOCENE",
        "EARLY JURASSIC",
        "EARLY MIOCENE",
        "EARLY PALEOCENE",
        "EARLY PERMIAN",
        "EARLY TRIASSIC",
        "EARLY/MID JURASSIC",
        "EOCENE",
        "INDETERMINATE",
        "JURASSIC",
        "JURASSIC/TRIASSIC",
        "LATE CARBONIFEROUS",
        "LATE CRETACEOUS",
        "LATE JURASSIC",
        "LATE PALEOCENE",
        "LATE PERMIAN",
        "LATE TRIASSIC",
        "MIDDLE JURASSIC",
        "MIDDLE TRIASSIC",
        "MIOCENE",
        "OLIGOCENE",
        "OXFORDIAN",
        "PALEOCENE",
        "PERMIAN",
        "PLEISTOCENE",
        "PLIOCENE",
        "PRE-DEVONIAN",
        "RHAETIAN",
        "TERTIARY",
        "TRIAS",
        "TRIASSIC",
    ];
    const PLAY_LABELS: &[&str] = &[
        "Basement",
        "Carboniferous",
        "Carboniferous-Permian",
        "Cretaceous",
        "Eocene",
        "Lower Cretaceous",
        "Lower-Middle Jurassic",
        "Middle Jurassic",
        "Middle-Upper Jurassic",
        "Miocen",
        "Paleocene",
        "Permian",
        "Permian-Palaeocene",
        "Pleistocene",
        "Post Palaeocene",
        "Pre-Jurassic",
        "Pre-Triassic",
        "Triassic, Lower",
        "Triassic, Lower-Middle",
        "Triassic, Lower-Middle Jurassic",
        "Triassic, Upper",
        "Upper Cretaceous",
        "Upper Jurassic",
    ];

    #[test]
    fn public_inventories_are_exhaustively_accounted_for() {
        assert_eq!(WELL_LABELS.len(), 38);
        assert_eq!(PLAY_LABELS.len(), 23);
        for label in WELL_LABELS.iter().chain(PLAY_LABELS) {
            if matches!(*label, "INDETERMINATE" | "Basement") {
                assert_eq!(normalize(label), None, "{label}");
            } else {
                assert!(normalize(label).is_some(), "missing mapping for {label}");
            }
        }
        assert_eq!(normalize(""), None);
        assert_eq!(normalize("not in the public inventory"), None);
    }

    #[test]
    fn broad_windows_and_stage_aliases_use_canonical_order() {
        assert_eq!(
            normalize("Lower-Middle Jurassic"),
            Some(&[LowerJurassic, MiddleJurassic][..])
        );
        assert_eq!(normalize("CALLOVIAN"), Some(MIDDLE_JURASSIC));
        assert_eq!(normalize("OXFORDIAN"), Some(UPPER_JURASSIC));
        assert_eq!(normalize("CAMPANIAN"), Some(UPPER_CRETACEOUS));
        assert_eq!(normalize("DANIAN"), Some(PALEOCENE));
        assert_eq!(normalize("RHAETIAN"), Some(UPPER_TRIASSIC));
        assert_eq!(
            normalize("Triassic, Lower-Middle Jurassic"),
            Some(TRIASSIC_LOWER_MIDDLE_JURASSIC)
        );
    }

    #[test]
    fn compatibility_distinguishes_containment_from_overlap() {
        assert_eq!(
            compatibility("EARLY JURASSIC", "Lower-Middle Jurassic"),
            Some(AgeMatch {
                compatibility: Compatibility::Full,
                atoms: vec![LowerJurassic],
            })
        );
        assert_eq!(
            compatibility("JURASSIC", "Lower-Middle Jurassic"),
            Some(AgeMatch {
                compatibility: Compatibility::Partial,
                atoms: vec![LowerJurassic, MiddleJurassic],
            })
        );
        assert_eq!(compatibility("CAMPANIAN", "Lower Cretaceous"), None);
        assert_eq!(compatibility("INDETERMINATE", "Cretaceous"), None);
    }
}

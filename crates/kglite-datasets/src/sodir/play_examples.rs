//! Curated field examples published on the SODIR play description pages.
//!
//! These rows are positive examples, not complete play membership. Callers
//! must match both public IDs and names exactly. A field may be published as
//! an example of more than one play; that evidence must not be propagated to
//! every constituent discovery without separate resolving evidence.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublishedFieldExample {
    pub field_name: &'static str,
    pub field_id: u64,
    pub play_name: &'static str,
    pub play_id: u64,
    pub source_label: &'static str,
    pub source_url: &'static str,
    pub accessed_on: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnresolvedFieldExample {
    pub source_label: &'static str,
    pub play_name: &'static str,
    pub play_id: u64,
    pub reason: &'static str,
    pub source_url: &'static str,
    pub accessed_on: &'static str,
}

const ACCESSED_ON: &str = "2026-09-08";
const NORTH_JURASSIC: &str =
    "https://www.sodir.no/en/facts/plays/north-sea/upper-triassic-to-middle-jurassic-plays/";
const NORTH_UPPER_JURASSIC: &str =
    "https://www.sodir.no/en/facts/plays/north-sea/upper-jurassic-plays/";
const NORTH_CRETACEOUS: &str = "https://www.sodir.no/en/facts/plays/north-sea/cretaceous-plays/";
const NORTH_PALEOCENE: &str = "https://www.sodir.no/en/facts/plays/north-sea/paleocene-plays/";
const NORTH_SUB_TRIASSIC: &str =
    "https://www.sodir.no/en/facts/plays/north-sea/sub-triassic-plays/";
const NORTH_SUB_UPPER_CRETACEOUS: &str =
    "https://www.sodir.no/en/facts/plays/north-sea/sub-upper-cretaceous-play/";
const NORWEGIAN_PALEOCENE: &str =
    "https://www.sodir.no/en/facts/plays/norwegian-sea/paleocene-plays/";
const NORWEGIAN_UPPER_CRETACEOUS: &str =
    "https://www.sodir.no/en/facts/plays/norwegian-sea/upper-cretaceous-plays/";
const NORWEGIAN_UPPER_JURASSIC: &str =
    "https://www.sodir.no/en/facts/plays/norwegian-sea/upper-jurassic-plays/";
const NORWEGIAN_MIDDLE_JURASSIC: &str =
    "https://www.sodir.no/en/facts/plays/norwegian-sea/middle-jurassic-play/";
const BARENTS_JURASSIC: &str =
    "https://www.sodir.no/en/facts/plays/barents-sea/lower-to-middle-jurassic-plays/";

macro_rules! example {
    ($field:literal, $field_id:literal, $play:literal, $play_id:literal, $url:ident) => {
        PublishedFieldExample {
            field_name: $field,
            field_id: $field_id,
            play_name: $play,
            play_id: $play_id,
            source_label: $field,
            source_url: $url,
            accessed_on: ACCESSED_ON,
        }
    };
    ($field:literal, $field_id:literal, $play:literal, $play_id:literal, $label:literal, $url:ident) => {
        PublishedFieldExample {
            field_name: $field,
            field_id: $field_id,
            play_name: $play,
            play_id: $play_id,
            source_label: $label,
            source_url: $url,
            accessed_on: ACCESSED_ON,
        }
    };
}

pub const PUBLISHED_FIELD_EXAMPLES: &[PublishedFieldExample] = &[
    example!(
        "STATFJORD",
        43658,
        "nru,jm-1",
        260,
        "Statfjord",
        NORTH_JURASSIC
    ),
    example!(
        "KVITEBJØRN",
        1036101,
        "nru,jm-1",
        260,
        "Kvitebjørn",
        NORTH_JURASSIC
    ),
    example!(
        "GULLFAKS",
        43686,
        "nru,jm-1",
        260,
        "Gullfaks",
        NORTH_JURASSIC
    ),
    example!("OSEBERG", 43625, "nru,jm-1", 260, "Oseberg", NORTH_JURASSIC),
    example!("SNORRE", 43718, "nru,jm-1", 260, "Snorre", NORTH_JURASSIC),
    example!("BRAGE", 43651, "nru,jm-1", 260, "Brage", NORTH_JURASSIC),
    example!(
        "VALEMON",
        20460969,
        "nru,jm-1",
        260,
        "Valemon",
        NORTH_JURASSIC
    ),
    example!("VISUND", 43745, "nru,jm-1", 260, "Visund", NORTH_JURASSIC),
    example!("KNARR", 20460988, "nru,jm-1", 260, "Knarr", NORTH_JURASSIC),
    example!(
        "VESLEFRIKK",
        43618,
        "nru,jm-1",
        260,
        "Veslefrikk",
        NORTH_JURASSIC
    ),
    example!(
        "MARTIN LINGE",
        21675447,
        "nru,jm-1",
        260,
        "Martin Linge",
        NORTH_JURASSIC
    ),
    example!(
        "SLEIPNER VEST",
        43457,
        "njl,jm-1",
        238,
        "Sleipner Vest",
        NORTH_JURASSIC
    ),
    example!(
        "RINGHORNE ØST",
        3505505,
        "njl,jm-1",
        238,
        "Ringhorne Øst",
        NORTH_JURASSIC
    ),
    example!("SIGYN", 1630100, "njl,jm-1", 238, "Sigyn", NORTH_JURASSIC),
    example!("VOLVE", 3420717, "njl,jm-1", 238, "Volve", NORTH_JURASSIC),
    example!("GUNGNE", 43464, "njl,jm-1", 238, "Gungne", NORTH_JURASSIC),
    example!("GAUPE", 18161341, "njl,jm-1", 238, "Gaupe", NORTH_JURASSIC),
    example!(
        "GINA KROG",
        23384544,
        "njl,jm-1",
        238,
        "Gina Krog",
        NORTH_JURASSIC
    ),
    example!(
        "IVAR AASEN",
        23384520,
        "njl,jm-1",
        238,
        "Ivar Aasen",
        NORTH_JURASSIC
    ),
    example!("YME", 43807, "njm-1", 241, "Yme", NORTH_JURASSIC),
    example!("TRYM", 18081500, "njl,jm-4", 140, "Trym", NORTH_JURASSIC),
    example!("TROLL", 46437, "nju-1", 242, "Troll", NORTH_UPPER_JURASSIC),
    example!("FRAM", 1578840, "nju-1", 242, "Fram", NORTH_UPPER_JURASSIC),
    example!("BRAGE", 43651, "nju-1", 242, "Brage", NORTH_UPPER_JURASSIC),
    example!("GJØA", 4467574, "nju-1", 242, "Gjøa", NORTH_UPPER_JURASSIC),
    example!(
        "STATFJORD NORD",
        43679,
        "nju-2",
        141,
        "Statfjord Nord",
        NORTH_UPPER_JURASSIC
    ),
    example!("ULA", 43800, "nju-3", 142, "Ula", NORTH_UPPER_JURASSIC),
    example!("GYDA", 43492, "nju-3", 142, "Gyda", NORTH_UPPER_JURASSIC),
    example!(
        "TAMBAR",
        1028599,
        "nju-3",
        142,
        "Tambar",
        NORTH_UPPER_JURASSIC
    ),
    example!("MIME", 43792, "nju-3", 142, "Mime", NORTH_UPPER_JURASSIC),
    example!("REV", 4467554, "nju-3", 142, "Rev", NORTH_UPPER_JURASSIC),
    example!(
        "ORMEN LANGE",
        2762452,
        "nhpc-1",
        231,
        "6305/5-1 Ormen Lange",
        NORWEGIAN_PALEOCENE
    ),
    // The page prints `bjl,mj-5`; play.csv identifies the same play (ID 165)
    // as `bjl,jm-5`, which is the canonical lookup name used here.
    example!(
        "SNØHVIT",
        2053062,
        "bjl,jm-5",
        165,
        "Snøhvit",
        BARENTS_JURASSIC
    ),
    example!(
        "GOLIAT",
        5774394,
        "bjl,jm-5",
        165,
        "Goliat",
        BARENTS_JURASSIC
    ),
    example!(
        "SKARV",
        4704482,
        "nhku-2",
        131,
        "Skarv",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    example!(
        "MARULK",
        18212090,
        "nhku-2",
        131,
        "Marulk",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    example!(
        "AASTA HANSTEEN",
        23395946,
        "nhku-4",
        133,
        "Aasta Hansteen",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    example!(
        "FENJA",
        31164879,
        "nhju-1",
        127,
        "Fenja",
        NORWEGIAN_UPPER_JURASSIC
    ),
    example!(
        "DRAUGEN",
        43758,
        "nhju-2",
        128,
        "Draugen",
        NORWEGIAN_UPPER_JURASSIC
    ),
    example!(
        "URD",
        2834734,
        "nhjm-1",
        225,
        "Urd",
        NORWEGIAN_MIDDLE_JURASSIC
    ),
    example!(
        "SKULD",
        21350124,
        "nhjm-1",
        225,
        "Skuld",
        NORWEGIAN_MIDDLE_JURASSIC
    ),
    example!("EMBLA", 43534, "npl-2", 259, "Embla", NORTH_SUB_TRIASSIC),
    example!("EKOFISK", 43506, "nku-2", 244, "Ekofisk", NORTH_CRETACEOUS),
    example!("ELDFISK", 43527, "nku-2", 244, "Eldfisk", NORTH_CRETACEOUS),
    example!("VALHALL", 43548, "nku-2", 244, "Valhall", NORTH_CRETACEOUS),
    example!("HOD", 43485, "nku-2", 244, "Hod", NORTH_CRETACEOUS),
    example!(
        "SLEIPNER ØST",
        43478,
        "npc-1",
        251,
        "Sleipner Øst",
        NORTH_PALEOCENE
    ),
    example!("HEIMDAL", 43590, "npc-1", 251, "Heimdal", NORTH_PALEOCENE),
    example!("BALDER", 43562, "npc-1", 251, "Balder", NORTH_PALEOCENE),
    example!("JOTUN", 43604, "npc-1", 251, "Jotun", NORTH_PALEOCENE),
    example!("GRANE", 1035937, "npc-1", 251, "Grane", NORTH_PALEOCENE),
    example!("ALVHEIM", 2845712, "npc-1", 251, "Alvheim", NORTH_PALEOCENE),
    example!("VOLUND", 4380167, "npc-1", 251, "Volund", NORTH_PALEOCENE),
    example!("BØYLA", 22492497, "npc-1", 251, "Bøyla", NORTH_PALEOCENE),
    example!("SVALIN", 22507971, "npc-1", 251, "Svalin", NORTH_PALEOCENE),
    example!(
        "EDVARD GRIEG",
        21675433,
        "nsbku-1",
        261,
        "Edvard Grieg",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
    example!(
        "JOHAN SVERDRUP",
        26376286,
        "nsbku-1",
        261,
        "Johan Sverdrup",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
];

pub const UNRESOLVED_FIELD_EXAMPLES: &[UnresolvedFieldExample] = &[UnresolvedFieldExample {
    source_label: "Ærfugl",
    play_name: "nhku-2",
    play_id: 131,
    reason: "no exact current field name; public field.csv contains ÆRFUGL NORD",
    source_url: NORWEGIAN_UPPER_CRETACEOUS,
    accessed_on: ACCESSED_ON,
}];

pub fn examples_for_field(
    field_name: &str,
    field_id: u64,
) -> impl Iterator<Item = &'static PublishedFieldExample> + '_ {
    PUBLISHED_FIELD_EXAMPLES
        .iter()
        .filter(move |example| example.field_name == field_name && example.field_id == field_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn majors_have_the_published_examples() {
        let expected = [
            ("STATFJORD", 43658, "nru,jm-1"),
            ("OSEBERG", 43625, "nru,jm-1"),
            ("TROLL", 46437, "nju-1"),
            ("ORMEN LANGE", 2762452, "nhpc-1"),
        ];
        for (field, field_id, play) in expected {
            assert_eq!(
                examples_for_field(field, field_id)
                    .map(|example| example.play_name)
                    .collect::<Vec<_>>(),
                vec![play]
            );
        }
    }

    #[test]
    fn matching_requires_both_exact_public_name_and_id() {
        assert_eq!(examples_for_field("Troll", 46437).count(), 0);
        assert_eq!(examples_for_field("TROLL", 1).count(), 0);
        assert_eq!(examples_for_field("TROLL", 46437).count(), 1);
    }

    #[test]
    fn duplicate_field_examples_are_preserved_as_source_facts() {
        let plays = examples_for_field("BRAGE", 43651)
            .map(|example| example.play_name)
            .collect::<BTreeSet<_>>();
        assert_eq!(plays, BTreeSet::from(["nju-1", "nru,jm-1"]));
    }

    #[test]
    fn curated_rows_are_unique_and_carry_provenance() {
        let mut keys = BTreeSet::new();
        for example in PUBLISHED_FIELD_EXAMPLES {
            assert!(keys.insert((example.field_id, example.play_id)));
            assert!(example
                .source_url
                .starts_with("https://www.sodir.no/en/facts/plays/"));
            assert_eq!(example.accessed_on, ACCESSED_ON);
        }
        assert_eq!(keys.len(), 57);
        assert_eq!(UNRESOLVED_FIELD_EXAMPLES.len(), 1);
    }
}

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
pub struct PublishedDiscoveryExample {
    pub discovery_name: &'static str,
    pub discovery_id: u64,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnresolvedDiscoveryExample {
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

macro_rules! discovery_example {
    ($name:literal, $id:literal, $play:literal, $play_id:literal, $label:literal, $url:ident) => {
        PublishedDiscoveryExample {
            discovery_name: $name,
            discovery_id: $id,
            play_name: $play,
            play_id: $play_id,
            source_label: $label,
            source_url: $url,
            accessed_on: ACCESSED_ON,
        }
    };
}

pub const PUBLISHED_DISCOVERY_EXAMPLES: &[PublishedDiscoveryExample] = &[
    discovery_example!("30/10-6", 44366, "nru,jm-1", 260, "30/10-6", NORTH_JURASSIC),
    discovery_example!(
        "34/4-10 (Beta Brent)",
        1030530,
        "nru,jm-1",
        260,
        "34/4-10",
        NORTH_JURASSIC
    ),
    discovery_example!(
        "15/3-4 (Sigrun)",
        43886,
        "njl,jm-1",
        238,
        "15/3-4",
        NORTH_JURASSIC
    ),
    discovery_example!(
        "15/5-2 Eirin",
        43898,
        "njl,jm-1",
        238,
        "15/5-2",
        NORTH_JURASSIC
    ),
    discovery_example!("15/12-8", 43874, "njl,jm-1", 238, "15/12-8", NORTH_JURASSIC),
    discovery_example!(
        "25/2-4 Lille-Frigg",
        44240,
        "njl,jm-1",
        238,
        "25/2-4",
        NORTH_JURASSIC
    ),
    discovery_example!("25/6-1", 44288, "njl,jm-1", 238, "25/6-1", NORTH_JURASSIC),
    discovery_example!(
        "17/12-1 Vette",
        43964,
        "njm-1",
        241,
        "17/12-1",
        NORTH_JURASSIC
    ),
    discovery_example!(
        "17/12-2 (Brisling)",
        43970,
        "njm-1",
        241,
        "17/12-2",
        NORTH_JURASSIC
    ),
    discovery_example!(
        "18/10-1 (Mackerel)",
        43982,
        "njm-1",
        241,
        "18/10-1",
        NORTH_JURASSIC
    ),
    discovery_example!("17/3-1", 43976, "njm-1", 241, "17/3-1", NORTH_JURASSIC),
    discovery_example!(
        "30/9-10 Oseberg Sør",
        44468,
        "nju-1",
        242,
        "30/9-10",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "35/9-7 Nova",
        21667341,
        "nju-1",
        242,
        "35/9-7",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "34/7-21 Borg",
        44690,
        "nju-2",
        141,
        "34/7-21",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "34/7-23 S",
        44702,
        "nju-2",
        141,
        "34/7-23 S",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "34/7-25 S",
        44708,
        "nju-2",
        141,
        "34/7-25 S",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "2/2-1 (Møyfrid)",
        44024,
        "nju-3",
        142,
        "2/2-1",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "2/4-21 Fenris",
        21669700,
        "nju-3",
        142,
        "2/4-21",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "2/7-29",
        44144,
        "nju-3",
        142,
        "2/7-29",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "8/10-4 S Oda",
        21118177,
        "nju-3",
        142,
        "8/10-4 S",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "25/7-2",
        44294,
        "nju-3",
        142,
        "25/7-2",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "25/10-8 Hanz",
        44198,
        "nju-3",
        142,
        "25/10-8",
        NORTH_UPPER_JURASSIC
    ),
    discovery_example!(
        "35/3-2 Agat",
        44768,
        "nkl-2",
        243,
        "35/3-2",
        NORTH_CRETACEOUS
    ),
    discovery_example!(
        "35/3-7 S",
        17221700,
        "nkl-2",
        243,
        "35/3-7 S",
        NORTH_CRETACEOUS
    ),
    PublishedDiscoveryExample {
        discovery_name: "35/9-3 (Gjøa Nord)",
        discovery_id: 45651,
        play_name: "nkl-2",
        play_id: 243,
        source_label: "35/9-3",
        source_url: NORTH_CRETACEOUS,
        accessed_on: ACCESSED_ON,
    },
    PublishedDiscoveryExample {
        discovery_name: "35/9-3 (Gjøa Nord)",
        discovery_id: 45651,
        play_name: "nku-5",
        play_id: 247,
        source_label: "35/9-3",
        source_url: NORTH_CRETACEOUS,
        accessed_on: ACCESSED_ON,
    },
    discovery_example!(
        "2/5-4 (Siv)",
        44096,
        "nku-3",
        245,
        "2/5-4",
        NORTH_CRETACEOUS
    ),
    discovery_example!(
        "2/5-11 (Tjatse)",
        46293,
        "nku-3",
        245,
        "2/5-11",
        NORTH_CRETACEOUS
    ),
    discovery_example!(
        "2/6-5 (Sundal)",
        44108,
        "nku-3",
        245,
        "2/6-5",
        NORTH_CRETACEOUS
    ),
    discovery_example!(
        "16/2-3 (Ragnarock)",
        4527337,
        "nku-4",
        246,
        "Ragnarock",
        NORTH_CRETACEOUS
    ),
    discovery_example!(
        "16/4-4 (Biotitt)",
        4445817,
        "npc-1",
        251,
        "Biotitt",
        NORTH_PALEOCENE
    ),
    discovery_example!(
        "24/12-3 S",
        44168,
        "npc-1",
        251,
        "24/12-3 S",
        NORTH_PALEOCENE
    ),
    discovery_example!(
        "25/4-7 Alvheim",
        2420578,
        "npc-1",
        251,
        "25/4-7",
        NORTH_PALEOCENE
    ),
    discovery_example!(
        "25/5-5 (Tir)",
        44282,
        "npc-1",
        251,
        "25/5-5",
        NORTH_PALEOCENE
    ),
    discovery_example!("25/7-5", 44306, "npc-1", 251, "25/7-5", NORTH_PALEOCENE),
    discovery_example!("1/2-1 Blane", 43814, "npc-2", 145, "1/2-1", NORTH_PALEOCENE),
    discovery_example!(
        "1/3-6 Oselvar",
        43832,
        "npc-2",
        145,
        "1/3-6",
        NORTH_PALEOCENE
    ),
    discovery_example!(
        "1/5-2 Flyndre",
        43838,
        "npc-2",
        145,
        "1/5-2",
        NORTH_PALEOCENE
    ),
    discovery_example!(
        "2/6-7 S (Othello)",
        42002508,
        "npc-3",
        252,
        "Othello",
        NORTH_PALEOCENE
    ),
    discovery_example!("2/7-22", 44138, "npl-2", 259, "2/7-22", NORTH_SUB_TRIASSIC),
    discovery_example!("2/7-31", 105311, "npl-2", 259, "2/7-31", NORTH_SUB_TRIASSIC),
    discovery_example!(
        "16/1-4",
        3000757,
        "nsbku-1",
        261,
        "16/1-4",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "16/1-12 Troldhaugen",
        17196400,
        "nsbku-1",
        261,
        "16/1-12",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "16/2-4",
        17237080,
        "nsbku-1",
        261,
        "16/2-4",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "16/2-5 (P-Graben)",
        5490457,
        "nsbku-1",
        261,
        "16/2-5",
        NORTH_SUB_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6406/12-3 A (Bue)",
        24689696,
        "nhju-1",
        127,
        "6406/12-3 A",
        NORWEGIAN_UPPER_JURASSIC
    ),
    discovery_example!(
        "6407/5-2 S (Cortina)",
        21076978,
        "nhju-1",
        127,
        "Cortina",
        NORWEGIAN_UPPER_JURASSIC
    ),
    discovery_example!(
        "6407/6-7 S (Harepus)",
        5506872,
        "nhju-1",
        127,
        "Harepus",
        NORWEGIAN_UPPER_JURASSIC
    ),
    discovery_example!(
        "6306/5-1",
        44828,
        "nhpc-1",
        231,
        "6306/5-1",
        NORWEGIAN_PALEOCENE
    ),
    discovery_example!(
        "6302/6-1 (Tulipan)",
        3505670,
        "nhpc-4",
        134,
        "Tulipan",
        NORWEGIAN_PALEOCENE
    ),
    discovery_example!(
        "6706/6-1 (Hvitveis)",
        2450045,
        "nhpc-4",
        134,
        "Hvitveis",
        NORWEGIAN_PALEOCENE
    ),
    discovery_example!(
        "6405/7-1 (Ellida)",
        2472532,
        "nhku-2",
        131,
        "Ellida",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6405/10-1 (Midnattsol)",
        4527324,
        "nhku-2",
        131,
        "Midnattsol",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6407/1-6 S (Rodriguez)",
        23137754,
        "nhku-2",
        131,
        "Rodriguez",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6407/1-7 (Solberg)",
        24670839,
        "nhku-2",
        131,
        "Solberg",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6607/12-3",
        22528465,
        "nhku-2",
        131,
        "6607/12-3",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6603/12-1 (Gro)",
        5798448,
        "nhku-4",
        133,
        "Gro",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6604/5-1 (Balderbrå)",
        31164612,
        "nhku-4",
        133,
        "Balderbrå",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6605/8-1",
        3490643,
        "nhku-4",
        133,
        "6605/8-1",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6705/10-1 Irpa",
        5466760,
        "nhku-4",
        133,
        "Irpa",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6706/11-2 (Gymir)",
        26376004,
        "nhku-4",
        133,
        "Gymir",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6706/12-1",
        5010082,
        "nhku-4",
        133,
        "6706/12-1",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6706/12-2 (Snefrid Nord)",
        26311166,
        "nhku-4",
        133,
        "Snefrid Nord",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6706/12-3 (Roald Rygg)",
        26336800,
        "nhku-4",
        133,
        "Roald Rygg",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6707/10-1 Aasta Hansteen",
        44954,
        "nhku-4",
        133,
        "Aasta Hansteen",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6707/10-2 S",
        5081127,
        "nhku-4",
        133,
        "6707/10-2 S",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6707/10-2 A",
        5125172,
        "nhku-4",
        133,
        "6707/10-2 A",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "6707/10-3 S (Ivory)",
        25594096,
        "nhku-4",
        133,
        "Ivory",
        NORWEGIAN_UPPER_CRETACEOUS
    ),
    discovery_example!(
        "7019/1-1",
        1340799,
        "bjl,jm-6",
        166,
        "7019/1-1",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7119/12-3",
        44996,
        "bjl,jm-6",
        166,
        "7119/12-3",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7219/8-2 (Iskrystall)",
        23762803,
        "bjl,jm-6",
        166,
        "Iskrystall",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7220/7-1 (Havis)",
        21334480,
        "bjl,jm-6",
        166,
        "Havis",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7220/7-3 S (Drivis)",
        24624378,
        "bjl,jm-6",
        166,
        "Drivis",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7220/8-1 Johan Castberg",
        20441001,
        "bjl,jm-6",
        166,
        "Johan Castberg",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7220/10-1 (Salina)",
        22468264,
        "bjl,jm-6",
        166,
        "Salina",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7124/3-1 (Bamse)",
        45074,
        "bjl,jm-7",
        167,
        "Bamse",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7125/4-1 (Nucula)",
        4445929,
        "bjl,jm-7",
        167,
        "Nucula",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7225/3-1 (Norvarg)",
        20499790,
        "bjl,jm-7",
        167,
        "Norvarg",
        BARENTS_JURASSIC
    ),
    discovery_example!(
        "7324/8-1 (Wisting)",
        23761893,
        "bjl,jm-7",
        167,
        "Wisting",
        BARENTS_JURASSIC
    ),
];

pub const UNRESOLVED_DISCOVERY_EXAMPLES: &[UnresolvedDiscoveryExample] = &[
    UnresolvedDiscoveryExample {
        source_label: "6406/12-4 S",
        play_name: "nhju-1",
        play_id: 127,
        reason: "no exact current discovery name match",
        source_url: NORWEGIAN_UPPER_JURASSIC,
        accessed_on: ACCESSED_ON,
    },
    UnresolvedDiscoveryExample {
        source_label: "6407/8-6 A",
        play_name: "nhju-1",
        play_id: 127,
        reason:
            "current discovery is 6407/8-6 Bauge; the published A qualifier cannot be broadened",
        source_url: NORWEGIAN_UPPER_JURASSIC,
        accessed_on: ACCESSED_ON,
    },
    UnresolvedDiscoveryExample {
        source_label: "6506/12-3",
        play_name: "nhku-2",
        play_id: 131,
        reason: "label resolves to multiple current qualified discoveries",
        source_url: NORWEGIAN_UPPER_CRETACEOUS,
        accessed_on: ACCESSED_ON,
    },
    UnresolvedDiscoveryExample {
        source_label: "7219/8-1",
        play_name: "bjl,jm-6",
        play_id: 166,
        reason: "no exact current discovery name match",
        source_url: BARENTS_JURASSIC,
        accessed_on: ACCESSED_ON,
    },
];

pub fn examples_for_field(
    field_name: &str,
    field_id: u64,
) -> impl Iterator<Item = &'static PublishedFieldExample> + '_ {
    PUBLISHED_FIELD_EXAMPLES
        .iter()
        .filter(move |example| example.field_name == field_name && example.field_id == field_id)
}

pub fn examples_for_discovery(
    discovery_name: &str,
    discovery_id: u64,
) -> impl Iterator<Item = &'static PublishedDiscoveryExample> + '_ {
    PUBLISHED_DISCOVERY_EXAMPLES.iter().filter(move |example| {
        example.discovery_name == discovery_name && example.discovery_id == discovery_id
    })
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

    #[test]
    fn published_discovery_can_be_an_example_of_multiple_plays() {
        let plays = examples_for_discovery("35/9-3 (Gjøa Nord)", 45651)
            .map(|example| (example.play_name, example.play_id))
            .collect::<BTreeSet<_>>();
        assert_eq!(plays, BTreeSet::from([("nkl-2", 243), ("nku-5", 247)]));
    }

    #[test]
    fn discovery_matching_requires_both_exact_public_name_and_id() {
        assert_eq!(examples_for_discovery("35/9-3", 45651).count(), 0);
        assert_eq!(examples_for_discovery("35/9-3 (Gjøa Nord)", 1).count(), 0);
        assert_eq!(
            examples_for_discovery("35/9-3 (Gjøa Nord)", 45651).count(),
            2
        );
    }

    #[test]
    fn discovery_rows_are_unique_and_carry_provenance() {
        let mut keys = BTreeSet::new();
        for example in PUBLISHED_DISCOVERY_EXAMPLES {
            assert!(keys.insert((example.discovery_id, example.play_id)));
            assert!(example
                .source_url
                .starts_with("https://www.sodir.no/en/facts/plays/"));
            assert_eq!(example.accessed_on, ACCESSED_ON);
        }
        assert_eq!(keys.len(), 79);
        assert_eq!(UNRESOLVED_DISCOVERY_EXAMPLES.len(), 4);
    }
}

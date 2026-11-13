//! The game's own files. A small themed set of its item icons is vendored in
//! `assets/instance-icons/` for instance pictures: The Indie Stone's terms allow distributing
//! parts of the base files as long as that doesn't make the game playable
//! (`research/indiestone/POLICY.md`). No Spiffo or other official branding goes in the set.

// Only the test that regenerates the vendored set reads packs; the app ships the PNGs.
#[cfg(test)]
#[path = "pack.model.rs"]
mod pack;

/// The themes and their items, plus the PNG bytes of each, from one list so the two can't
/// drift apart. Each file is the game's 32 px icon scaled 4x with nearest neighbour.
macro_rules! themes {
    ($($theme:literal => [$($item:literal),* $(,)?]),* $(,)?) => {
        pub const THEMES: &[(&str, &[&str])] = &[$(($theme, &[$($item),*])),*];

        /// The vendored icon for `item` (`Item_Axe`), if it is in the set.
        pub fn icon(item: &str) -> Option<&'static [u8]> {
            match item {
                $($($item => Some(include_bytes!(
                    concat!("../../../../assets/instance-icons/", $item, ".png")
                )),)*)*
                _ => None,
            }
        }
    };
}

themes! {
    "Weapons" => [
        "Item_Axe", "Item_BaseballBat", "Item_BaseballBatNails", "Item_Crowbar",
        "Item_Katana", "Item_Machete", "Item_Shotgun", "Item_HuntingRifle",
    ],
    "Tools" => [
        "Item_Hammer", "Item_Sledgehammer2", "Item_Screwdriver", "Item_Wrench",
        "Item_PipeWrench", "Item_Shovel", "Item_Toolbox", "Item_FishingRod",
    ],
    "Food" => [
        "Item_Beans", "Item_Cereal", "Item_Pan", "Item_Apple",
        "Item_Bread_fresh", "Item_Burger", "Item_Pop2", "Item_CannedCornedBeef",
    ],
    "Medical" => [
        "Item_FirstAid", "Item_Bandage", "Item_Splint", "Item_Pills2",
        "Item_Pills4", "Item_DuffelbagMedical",
    ],
    "Survival" => [
        "Item_Duffelbag", "Item_Backpack", "Item_SleepingBag", "Item_TentLarge",
        "Item_Map", "Item_Petrol", "Item_Flashlight", "Item_Lighter",
    ],
    "Everyday" => [
        "Item_HamRadioCivilian", "Item_WalkieTalkieCivilian", "Item_Generator", "Item_Battery",
        "Item_CarKey", "Item_GuitarAcoustic", "Item_Book1", "Item_CandleLit",
    ],
}

/// Every item in the set, in theme order: what Randomize picks from.
pub fn items() -> impl Iterator<Item = &'static str> {
    THEMES.iter().flat_map(|(_, items)| items.iter().copied())
}

/// Asset path the app's asset source serves the icon at.
pub fn icon_path(item: &str) -> String {
    format!("instance-icons/{item}.png")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_listed_item_has_an_icon_and_none_is_branded() {
        for item in items() {
            let bytes = icon(item).expect(item);
            assert!(bytes.starts_with(b"\x89PNG"), "{item} is a PNG");
            assert!(
                !item.to_lowercase().contains("spiffo"),
                "{item}: no official branding"
            );
        }
        assert!(icon("Item_Nope").is_none());
    }

    /// Regenerates `assets/instance-icons/` from an installed build, for when the set changes:
    /// `LULO_PZ_PACKS=<build>/projectzomboid/media/texturepacks cargo test -p lulo -- --ignored vendors`
    #[test]
    #[ignore]
    fn vendors_the_item_icons() {
        let packs =
            std::path::PathBuf::from(std::env::var("LULO_PZ_PACKS").expect("LULO_PZ_PACKS"));
        let out = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/instance-icons");
        let wanted: Vec<&str> = items().collect();
        let mut written = 0;
        for name in ["UI.pack", "UI2.pack"] {
            pack::read(&packs.join(name))
                .unwrap()
                .for_each(|sprite, image| {
                    if wanted.contains(&sprite.name.as_str()) {
                        let (w, h) = image.dimensions();
                        image::imageops::resize(
                            &image,
                            w * 4,
                            h * 4,
                            image::imageops::FilterType::Nearest,
                        )
                        .save(format!("{out}/{}.png", sprite.name))
                        .unwrap();
                        written += 1;
                    }
                })
                .unwrap();
        }
        assert!(
            written >= wanted.len(),
            "{written} of {} icons found",
            wanted.len()
        );
    }
}

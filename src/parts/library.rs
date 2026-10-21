use crate::engine::library::Library;

const EMBEDDED: &[&str] = &[
    include_str!("../../components/newport_sfpx040.toml"),
    include_str!("../../components/newport_sfpx040_ar14.toml"),
    include_str!("../../components/thorlabs_ac127_019_a.toml"),
    include_str!("../../components/thorlabs_ac254_050_a.toml"),
    include_str!("../../components/edmund_47_662.toml"),
    include_str!("../../components/thorlabs_mpd01m9.toml"),
    include_str!("../../components/thorlabs_p50d.toml"),
    include_str!("../../components/thorlabs_p25d.toml"),
    include_str!("../../components/edmund_56_282.toml"),
    include_str!("../../components/thorlabs_gt25_03.toml"),
    include_str!("../../components/edmund_grating_300.toml"),
    include_str!("../../components/sample_mirror.toml"),
    include_str!("../../components/sample_metal.toml"),
    include_str!("../../components/sample_pcb.toml"),
    include_str!("../../components/sample_matte.toml"),
];

pub fn library() -> Library {
    Library::load(EMBEDDED, "components")
}

#[cfg(test)]
mod tests {
    #[test]
    fn components_are_listed_once_and_all_parse() {
        let lib = super::library();
        assert_eq!(lib.comps.len(), super::EMBEDDED.len());
        assert!(lib.errors.is_empty(), "{:?}", lib.errors);
    }
}

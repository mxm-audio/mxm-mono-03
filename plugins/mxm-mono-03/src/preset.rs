//! Presets: this instrument's factory set, and what the collection's preset crate needs of it.
//!
//! The format, the library on disk, favourites, the loaded identity and the app-bar controls are
//! `mxm-preset`'s — one crate for every instrument and effect, extracted from the five verbatim
//! copies this file used to be one of (`plugins/AGENTS.md`, *A preset is parameter values*). What
//! is left here is what only this instrument knows: its id, its parameters, and its sounds.

use std::sync::RwLock;

pub use mxm_preset::{
    Category, Entry, INIT_NAME, Library, Loaded, Origin, Preset, PresetIdentity, Refused, Value,
    factory, loaded, mark_loaded, mark_none, read_favourites, snapshot, write_favourites,
};

use crate::params::MxmMono03Params;

impl mxm_preset::Instrument for MxmMono03Params {
    fn clap_id(&self) -> &'static str {
        crate::CLAP_ID
    }

    /// In declaration order, from the one list the editor draws from, then every route: **presets
    /// carry routing**, presences included.
    fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        crate::editor::sections::all_parameters(self)
            .into_iter()
            .map(|bound| (bound.id, bound.param))
            .chain(self.routes.parameters())
            .collect()
    }

    fn identity(&self) -> &RwLock<PresetIdentity> {
        &self.preset
    }

    fn factory_files(&self) -> &'static [(&'static str, &'static str)] {
        FACTORY_FILES
    }
}

/// The factory set, compiled in.
///
/// **Fifty files, and Init is not one of them** — see [`Preset::init`]. These fifty are
/// *content*: a sound nobody can read is a sound nobody can learn from, so they are files rather
/// than code.
pub const FACTORY_FILES: &[(&str, &str)] = &[
    ("Acid line", include_str!("../presets/acid-line.json")),
    ("Squelch", include_str!("../presets/squelch.json")),
    ("Deep acid", include_str!("../presets/deep-acid.json")),
    ("Square acid", include_str!("../presets/square-acid.json")),
    ("Rubber bass", include_str!("../presets/rubber-bass.json")),
    ("Round bass", include_str!("../presets/round-bass.json")),
    ("Hollow knock", include_str!("../presets/hollow-knock.json")),
    ("Dark drone", include_str!("../presets/dark-drone.json")),
    ("Bright saw", include_str!("../presets/bright-saw.json")),
    ("Buzz lead", include_str!("../presets/buzz-lead.json")),
    ("Snap pluck", include_str!("../presets/snap-pluck.json")),
    ("Chirp", include_str!("../presets/chirp.json")),
    ("Slow sweep", include_str!("../presets/slow-sweep.json")),
    ("Overdriven", include_str!("../presets/overdriven.json")),
    ("Three diode", include_str!("../presets/three-diode.json")),
    ("Lazy slide", include_str!("../presets/lazy-slide.json")),
    ("Soft square", include_str!("../presets/soft-square.json")),
    ("Hard accent", include_str!("../presets/hard-accent.json")),
    (
        "Climbing accents",
        include_str!("../presets/climbing-accents.json"),
    ),
    ("Low rumble", include_str!("../presets/low-rumble.json")),
    ("Classic acid", include_str!("../presets/classic-acid.json")),
    ("Rolling bass", include_str!("../presets/rolling-bass.json")),
    ("Detroit stab", include_str!("../presets/detroit-stab.json")),
    ("Wet squelch", include_str!("../presets/wet-squelch.json")),
    ("Dry thump", include_str!("../presets/dry-thump.json")),
    (
        "Warm saw bass",
        include_str!("../presets/warm-saw-bass.json"),
    ),
    ("Woody square", include_str!("../presets/woody-square.json")),
    ("Long acid", include_str!("../presets/long-acid.json")),
    ("Short acid", include_str!("../presets/short-acid.json")),
    (
        "Hoover accent",
        include_str!("../presets/hoover-accent.json"),
    ),
    ("Fizz", include_str!("../presets/fizz.json")),
    ("Sub thud", include_str!("../presets/sub-thud.json")),
    ("Drive lead", include_str!("../presets/drive-lead.json")),
    ("Whistle lead", include_str!("../presets/whistle-lead.json")),
    ("Nasal lead", include_str!("../presets/nasal-lead.json")),
    ("Bright pluck", include_str!("../presets/bright-pluck.json")),
    ("Muted pluck", include_str!("../presets/muted-pluck.json")),
    ("Ping", include_str!("../presets/ping.json")),
    ("Bongo", include_str!("../presets/bongo.json")),
    ("Kick tick", include_str!("../presets/kick-tick.json")),
    ("Wood block", include_str!("../presets/wood-block.json")),
    ("Laser", include_str!("../presets/laser.json")),
    ("Filter howl", include_str!("../presets/filter-howl.json")),
    ("Slow open", include_str!("../presets/slow-open.json")),
    ("Hum drone", include_str!("../presets/hum-drone.json")),
    ("Accent run", include_str!("../presets/accent-run.json")),
    ("Slide run", include_str!("../presets/slide-run.json")),
    ("Gritty saw", include_str!("../presets/gritty-saw.json")),
    ("Clean square", include_str!("../presets/clean-square.json")),
    ("Mid acid", include_str!("../presets/mid-acid.json")),
];

#[cfg(test)]
mod tests {
    use super::*;
    use mxm_preset::user_root;

    fn params() -> MxmMono03Params {
        MxmMono03Params::default()
    }

    /// Prints every parameter as eleven `normalised=formatted` steps.
    ///
    /// A facility, not a test: designing a factory preset means choosing normalised values, and
    /// choosing them blind is how a preset ends up with a filter at 0.5 that nobody meant.
    ///
    /// ```text
    /// cargo test -p mxm-mono-03 --lib the_mapping_table -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "prints what each normalised value means, for preset design"]
    fn the_mapping_table() {
        let params = params();
        for bound in crate::editor::sections::all_parameters(&params) {
            let steps: Vec<String> = (0..=10)
                .map(|i| {
                    let v = i as f32 / 10.0;
                    format!("{v:.1}={}", bound.param.format(v))
                })
                .collect();
            eprintln!("{:<12} {}", bound.id, steps.join("  "));
        }
    }

    /// The factory sounds, as **overrides on the defaults**.
    ///
    /// Written as the handful of values that make each sound rather than as nineteen numbers
    /// apiece: a file full of defaults hides the three that matter. `write_the_factory_presets`
    /// turns each into a complete file, because the format takes no sparse overlays — an overlay's
    /// meaning would change the day the defaults were retuned.
    ///
    /// **Fifty, and Init is not one of them.** Init is generated from the parameter defaults and
    /// has no file at all; see [`Preset::init`].
    ///
    /// The values this instrument plays with are few: cutoff, resonance, decay and accent on the
    /// panel and the Env Mod route, so most designs are five numbers — which is faithful, because
    /// that is the machine. An Env Mod at normalised `n` is the route's amount at `(n + 1) / 2`
    /// (`plans/plan-mxm-mono-03-modulation.md` §4).
    const FACTORY_DESIGN: &[Design] = &[
        (
            "Acid line",
            Category::Bass,
            // the reference squelch: resonance and envelope doing the talking
            &[
                ("cutoff", 0.32),
                ("resonance", 0.85),
                ("mod_cutoff_env", 0.825),
                ("decay", 0.35),
                ("accent", 0.8),
            ],
        ),
        (
            "Squelch",
            Category::Bass,
            // further down the same road: nearly shut, nearly self-oscillating
            &[
                ("cutoff", 0.22),
                ("resonance", 0.95),
                ("mod_cutoff_env", 0.925),
                ("decay", 0.28),
                ("accent", 0.95),
            ],
        ),
        (
            "Deep acid",
            Category::Bass,
            // dark and rolling rather than screaming
            &[
                ("cutoff", 0.12),
                ("resonance", 0.7),
                ("mod_cutoff_env", 0.775),
                ("decay", 0.45),
                ("accent", 0.7),
            ],
        ),
        (
            "Square acid",
            Category::Bass,
            // the same line with the hollower source
            &[
                ("waveform", 1.0),
                ("cutoff", 0.3),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.32),
                ("accent", 0.8),
            ],
        ),
        (
            "Rubber bass",
            Category::Bass,
            // fast envelope, moderate resonance: it bounces
            &[
                ("waveform", 1.0),
                ("cutoff", 0.28),
                ("resonance", 0.55),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.22),
                ("accent", 0.5),
            ],
        ),
        (
            "Round bass",
            Category::Bass,
            // almost no resonance, so it sits under everything
            &[
                ("cutoff", 0.3),
                ("resonance", 0.12),
                ("mod_cutoff_env", 0.6),
                ("decay", 0.5),
            ],
        ),
        (
            "Hollow knock",
            Category::Percussion,
            // a short envelope on a square: percussive
            &[
                ("waveform", 1.0),
                ("cutoff", 0.38),
                ("resonance", 0.7),
                ("mod_cutoff_env", 0.75),
                ("decay", 0.15),
                ("accent", 0.65),
            ],
        ),
        (
            "Dark drone",
            Category::Drone,
            // the amp decay stretched, so held notes actually hold
            &[
                ("cutoff", 0.18),
                ("resonance", 0.05),
                ("mod_cutoff_env", 0.55),
                ("decay", 0.7),
                ("ampdecay", 0.56),
            ],
        ),
        (
            "Bright saw",
            Category::Bass,
            // the filter mostly open: the oscillator itself
            &[
                ("cutoff", 0.75),
                ("resonance", 0.25),
                ("mod_cutoff_env", 0.65),
                ("decay", 0.5),
            ],
        ),
        (
            "Buzz lead",
            Category::Lead,
            // open, resonant, and long
            &[
                ("cutoff", 0.9),
                ("resonance", 0.5),
                ("mod_cutoff_env", 0.6),
                ("decay", 0.6),
            ],
        ),
        (
            "Snap pluck",
            Category::Pluck,
            // the shortest useful envelope on both paths
            &[
                ("cutoff", 0.4),
                ("resonance", 0.45),
                ("mod_cutoff_env", 0.775),
                ("decay", 0.1),
                ("ampdecay", 0.19),
            ],
        ),
        (
            "Chirp",
            Category::Pluck,
            // envelope depth at maximum over a shut filter: all sweep
            &[
                ("cutoff", 0.1),
                ("resonance", 0.6),
                ("mod_cutoff_env", 1.0),
                ("decay", 0.05),
            ],
        ),
        (
            "Slow sweep",
            Category::Bass,
            // the longest decay, so each note is a filter ride
            &[
                ("cutoff", 0.15),
                ("resonance", 0.75),
                ("mod_cutoff_env", 0.9),
                ("decay", 0.9),
            ],
        ),
        (
            "Overdriven",
            Category::Bass,
            // the drive into the ladder raised: rougher everywhere
            &[
                ("cutoff", 0.3),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.35),
                ("accent", 0.85),
                ("drive", 0.86),
            ],
        ),
        (
            "Three diode",
            Category::Bass,
            // the other pole set, which is a different filter rather than a detuning
            &[
                ("filtermodel", 1.0),
                ("cutoff", 0.35),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.4),
                ("accent", 0.7),
            ],
        ),
        (
            "Lazy slide",
            Category::Bass,
            // the slide RC stretched, so tied notes pour rather than step
            &[
                ("cutoff", 0.3),
                ("resonance", 0.5),
                ("mod_cutoff_env", 0.725),
                ("decay", 0.4),
                ("slidetime", 0.63),
            ],
        ),
        (
            "Soft square",
            Category::Bass,
            // the mellow one: low corner, no resonance to catch the ear
            &[
                ("waveform", 1.0),
                ("cutoff", 0.25),
                ("resonance", 0.1),
                ("mod_cutoff_env", 0.575),
                ("decay", 0.45),
            ],
        ),
        (
            "Hard accent",
            Category::Bass,
            // accent depth at maximum: the dynamics are the sound
            &[
                ("cutoff", 0.28),
                ("resonance", 0.9),
                ("mod_cutoff_env", 0.85),
                ("decay", 0.3),
                ("accent", 1.0),
            ],
        ),
        (
            "Climbing accents",
            Category::Sequence,
            // the sweep's charge stretched, so runs of accents stack higher
            &[
                ("cutoff", 0.25),
                ("resonance", 0.85),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.3),
                ("accent", 0.9),
                ("accentsweep", 0.8),
            ],
        ),
        (
            "Low rumble",
            Category::Bass,
            // nearly shut and barely moving: texture rather than line
            &[
                ("cutoff", 0.1),
                ("resonance", 0.3),
                ("mod_cutoff_env", 0.675),
                ("decay", 0.55),
            ],
        ),
        (
            "Classic acid",
            Category::Bass,
            &[
                ("cutoff", 0.35),
                ("resonance", 0.75),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.4),
                ("accent", 0.75),
            ],
        ),
        (
            "Rolling bass",
            Category::Bass,
            &[
                ("cutoff", 0.28),
                ("resonance", 0.4),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.3),
                ("accent", 0.4),
                ("slidetime", 0.45),
            ],
        ),
        (
            "Detroit stab",
            Category::Bass,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.45),
                ("resonance", 0.7),
                ("mod_cutoff_env", 0.85),
                ("decay", 0.2),
                ("accent", 0.9),
                ("ampdecay", 0.4),
            ],
        ),
        (
            "Wet squelch",
            Category::Bass,
            &[
                ("cutoff", 0.2),
                ("resonance", 1.0),
                ("mod_cutoff_env", 0.95),
                ("decay", 0.35),
                ("accent", 0.9),
                ("accentsweep", 0.6),
            ],
        ),
        (
            "Dry thump",
            Category::Bass,
            &[
                ("cutoff", 0.25),
                ("resonance", 0.0),
                ("mod_cutoff_env", 0.65),
                ("decay", 0.2),
                ("ampdecay", 0.35),
                ("drive", 0.3),
            ],
        ),
        (
            "Warm saw bass",
            Category::Bass,
            &[
                ("cutoff", 0.4),
                ("resonance", 0.15),
                ("mod_cutoff_env", 0.625),
                ("decay", 0.6),
                ("drive", 0.55),
                ("ampdecay", 0.7),
            ],
        ),
        (
            "Woody square",
            Category::Bass,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.35),
                ("resonance", 0.3),
                ("mod_cutoff_env", 0.65),
                ("decay", 0.35),
                ("drive", 0.4),
            ],
        ),
        (
            "Long acid",
            Category::Bass,
            &[
                ("cutoff", 0.3),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.85),
                ("decay", 0.75),
                ("accent", 0.8),
                ("accentdecay", 0.6),
            ],
        ),
        (
            "Short acid",
            Category::Bass,
            &[
                ("cutoff", 0.35),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.825),
                ("decay", 0.12),
                ("accent", 0.85),
                ("ampdecay", 0.3),
            ],
        ),
        (
            "Hoover accent",
            Category::Bass,
            &[
                ("cutoff", 0.3),
                ("resonance", 0.7),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.4),
                ("accent", 1.0),
                ("accentsweep", 1.0),
                ("accentdecay", 0.7),
            ],
        ),
        (
            "Fizz",
            Category::Bass,
            &[
                ("cutoff", 0.5),
                ("resonance", 0.9),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.3),
                ("accent", 0.6),
                ("drive", 0.7),
            ],
        ),
        (
            "Sub thud",
            Category::Bass,
            &[
                ("cutoff", 0.08),
                ("resonance", 0.2),
                ("mod_cutoff_env", 0.625),
                ("decay", 0.3),
                ("ampdecay", 0.45),
            ],
        ),
        (
            "Drive lead",
            Category::Lead,
            &[
                ("cutoff", 0.7),
                ("resonance", 0.6),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.55),
                ("drive", 0.9),
                ("ampdecay", 0.8),
                ("accent", 0.5),
            ],
        ),
        (
            "Whistle lead",
            Category::Lead,
            &[
                ("cutoff", 0.65),
                ("resonance", 0.95),
                ("mod_cutoff_env", 0.65),
                ("decay", 0.7),
                ("ampdecay", 0.9),
                ("waveform", 1.0),
            ],
        ),
        (
            "Nasal lead",
            Category::Lead,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.55),
                ("resonance", 0.5),
                ("mod_cutoff_env", 0.675),
                ("decay", 0.5),
                ("ampdecay", 0.75),
                ("slidetime", 0.4),
            ],
        ),
        (
            "Bright pluck",
            Category::Pluck,
            &[
                ("cutoff", 0.6),
                ("resonance", 0.35),
                ("mod_cutoff_env", 0.75),
                ("decay", 0.12),
                ("ampdecay", 0.25),
            ],
        ),
        (
            "Muted pluck",
            Category::Pluck,
            &[
                ("cutoff", 0.2),
                ("resonance", 0.25),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.1),
                ("ampdecay", 0.2),
                ("drive", 0.3),
            ],
        ),
        (
            "Ping",
            Category::Pluck,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.45),
                ("resonance", 0.95),
                ("mod_cutoff_env", 0.65),
                ("decay", 0.15),
                ("ampdecay", 0.3),
            ],
        ),
        (
            "Bongo",
            Category::Percussion,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.35),
                ("resonance", 0.85),
                ("mod_cutoff_env", 0.8),
                ("decay", 0.08),
                ("ampdecay", 0.15),
                ("accent", 0.5),
            ],
        ),
        (
            "Kick tick",
            Category::Percussion,
            &[
                ("cutoff", 0.12),
                ("resonance", 0.5),
                ("mod_cutoff_env", 0.85),
                ("decay", 0.06),
                ("ampdecay", 0.18),
                ("ampattack", 0.0),
            ],
        ),
        (
            "Wood block",
            Category::Percussion,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.5),
                ("resonance", 0.9),
                ("mod_cutoff_env", 0.7),
                ("decay", 0.05),
                ("ampdecay", 0.1),
            ],
        ),
        (
            "Laser",
            Category::Fx,
            &[
                ("cutoff", 0.1),
                ("resonance", 0.95),
                ("mod_cutoff_env", 1.0),
                ("decay", 0.2),
                ("accent", 1.0),
                ("accentsweep", 1.0),
                ("ampdecay", 0.4),
            ],
        ),
        (
            "Filter howl",
            Category::Fx,
            &[
                ("cutoff", 0.15),
                ("resonance", 1.0),
                ("mod_cutoff_env", 0.6),
                ("decay", 0.9),
                ("ampdecay", 1.0),
                ("drive", 0.8),
            ],
        ),
        (
            "Slow open",
            Category::Drone,
            &[
                ("cutoff", 0.1),
                ("resonance", 0.4),
                ("mod_cutoff_env", 0.85),
                ("decay", 1.0),
                ("filterattack", 1.0),
                ("ampattack", 0.8),
                ("ampdecay", 1.0),
            ],
        ),
        (
            "Hum drone",
            Category::Drone,
            &[
                ("cutoff", 0.2),
                ("resonance", 0.0),
                ("mod_cutoff_env", 0.525),
                ("decay", 0.8),
                ("ampdecay", 1.0),
                ("drive", 0.6),
            ],
        ),
        (
            "Accent run",
            Category::Sequence,
            &[
                ("cutoff", 0.28),
                ("resonance", 0.8),
                ("mod_cutoff_env", 0.825),
                ("decay", 0.35),
                ("accent", 0.9),
                ("accentsweep", 0.3),
                ("accentdecay", 0.25),
            ],
        ),
        (
            "Slide run",
            Category::Sequence,
            &[
                ("cutoff", 0.3),
                ("resonance", 0.6),
                ("mod_cutoff_env", 0.75),
                ("decay", 0.4),
                ("slidetime", 0.9),
                ("accent", 0.6),
            ],
        ),
        (
            "Gritty saw",
            Category::Bass,
            &[
                ("cutoff", 0.35),
                ("resonance", 0.55),
                ("mod_cutoff_env", 0.75),
                ("decay", 0.4),
                ("accent", 0.7),
                ("drive", 1.0),
            ],
        ),
        (
            "Clean square",
            Category::Bass,
            &[
                ("waveform", 1.0),
                ("cutoff", 0.45),
                ("resonance", 0.05),
                ("mod_cutoff_env", 0.55),
                ("decay", 0.5),
                ("drive", 0.15),
            ],
        ),
        (
            "Mid acid",
            Category::Bass,
            &[
                ("cutoff", 0.5),
                ("resonance", 0.7),
                ("mod_cutoff_env", 0.75),
                ("decay", 0.4),
                ("accent", 0.7),
                ("filtermodel", 1.0),
            ],
        ),
    ];

    /// One designed sound: its name, its category, and the values that make it.
    type Design = (&'static str, Category, &'static [(&'static str, f32)]);

    /// Writes the fifty factory presets to `plugins/mxm-mono-03/presets/`.
    ///
    /// A facility, not a test — and the *only* thing that writes those files, so the numbers in
    /// `FACTORY_DESIGN` stay the readable statement of each sound and the JSON stays generated
    /// output. `every_factory_preset_covers_every_parameter` is what catches a file that has fallen
    /// behind a new parameter.
    ///
    /// ```text
    /// cargo test -p mxm-mono-03 --lib write_the_factory_presets -- --ignored --nocapture
    /// ```
    #[test]
    #[ignore = "writes the factory preset files"]
    fn write_the_factory_presets() {
        let params = params();
        let bindings = mxm_preset::Instrument::parameters(&params);

        for (name, category, overrides) in FACTORY_DESIGN {
            let mut preset = Preset::init(&params);
            preset.name = (*name).to_owned();
            preset.category = *category;

            for (id, v) in *overrides {
                let (_, param) = bindings
                    .iter()
                    .find(|(pid, _)| pid == id)
                    .unwrap_or_else(|| panic!("{name:?} names `{id}`, which is not a parameter"));
                preset.params.insert(
                    (*id).to_owned(),
                    Value {
                        v: *v,
                        text: param.format(*v),
                    },
                );
            }

            // From the manifest directory, not the working one: a test's cwd is the crate root
            // and not the workspace root, which is the sort of thing that only says so once.
            let file = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("presets")
                .join(format!("{}.json", name.to_lowercase().replace(' ', "-")));
            std::fs::write(&file, preset.to_json()).expect("write the preset");
            eprintln!("wrote {}", file.display());
        }
    }

    #[test]
    fn every_designed_preset_names_real_parameters() {
        // Runs by default, unlike the generator: a typo in `FACTORY_DESIGN` would otherwise only
        // surface the next time somebody regenerated the files, and silently leave that value at
        // its default in the meantime.
        let params = params();
        let bindings = mxm_preset::Instrument::parameters(&params);
        for (name, _category, overrides) in FACTORY_DESIGN {
            for (id, v) in *overrides {
                assert!(
                    bindings.iter().any(|(pid, _)| pid == id),
                    "{name:?} names `{id}`, which is not a parameter of this instrument"
                );
                assert!(
                    (0.0..=1.0).contains(v),
                    "{name:?} sets `{id}` to {v}, which is not a normalised value"
                );
            }
        }
    }

    #[test]
    fn the_factory_files_match_the_design_they_were_generated_from() {
        // The generator is `#[ignore]`d, so nothing forces it to have been run. This is what says
        // the shipped files are the current design rather than a stale one — the same class of
        // mistake as a stale `.clap` bundle, and just as quiet.
        for (name, _category, overrides) in FACTORY_DESIGN {
            let (_, text) = FACTORY_FILES
                .iter()
                .find(|(file_name, _)| file_name == name)
                .unwrap_or_else(|| panic!("no factory file for {name:?}"));
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");

            for (id, v) in *overrides {
                let value = preset
                    .params
                    .get(*id)
                    .unwrap_or_else(|| panic!("{name:?} is missing `{id}`"));
                assert!(
                    (value.v - v).abs() < 1e-6,
                    "{name:?} ships `{id}` at {} but is designed at {v} — regenerate the files",
                    value.v
                );
            }
        }
    }

    #[test]
    fn the_user_root_is_under_this_instruments_own_id() {
        // Namespaced by CLAP id so another instrument's presets cannot appear in this one's list.
        let Some(root) = user_root(crate::CLAP_ID) else {
            return;
        };
        assert!(root.ends_with("presets"));
        assert!(root.to_string_lossy().contains(crate::CLAP_ID));
    }

    #[test]
    fn every_factory_preset_can_be_heard() {
        // This instrument's oscillator always sounds, so audibility is the volume and the filter.
        // A preset with the output at nothing, or the filter shut with no envelope to open it,
        // loads without complaint and reads as the instrument being broken.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            let v = |id: &str| preset.params.get(id).map_or(0.0, |value| value.v);
            assert!(v("volume") > 0.05, "{name:?} is turned down to nothing");
            assert!(
                v("cutoff") > 0.05 || v("mod_cutoff_env") > 0.6,
                "{name:?} has the filter shut and nothing to open it"
            );
        }
    }

    #[test]
    fn no_two_factory_presets_are_the_same_sound() {
        // Fifty is enough that a copied-and-edited design could lose its edit unnoticed.
        for (index, (name, text)) in FACTORY_FILES.iter().enumerate() {
            let a = Preset::parse(text, crate::CLAP_ID).expect("parses");
            for (other, text) in &FACTORY_FILES[index + 1..] {
                let b = Preset::parse(text, crate::CLAP_ID).expect("parses");
                assert_ne!(a.params, b.params, "{name:?} and {other:?} are identical");
            }
        }
    }

    #[test]
    fn every_factory_preset_has_a_category() {
        // A sound is saved with its category (the owner's rule, 2026-09-04), and the factory set
        // is where a person first sees what the categories mean. *Uncategorised* is for files
        // written before the field existed, not for sounds this instrument ships.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            assert_ne!(
                preset.category,
                Category::Uncategorised,
                "factory preset {name:?} has no category"
            );
        }
    }

    #[test]
    fn every_factory_preset_parses_and_is_for_this_instrument() {
        // A malformed factory preset is a build mistake, not a user's, so it is caught here rather
        // than skipped quietly in the browser.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID)
                .unwrap_or_else(|e| panic!("factory preset {name:?} does not parse: {e}"));
            assert_eq!(preset.name, *name, "the file's name must match its listing");
        }
    }

    #[test]
    fn every_factory_preset_covers_every_parameter() {
        // The one that catches a factory preset written before a parameter existed: it would load
        // and quietly leave that parameter wherever the last patch left it.
        let params = params();
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            let (_, problems) = preset.resolve(&params);
            assert!(
                problems.is_empty(),
                "factory preset {name:?} is incomplete: {problems:?}"
            );
        }
    }

    #[test]
    fn the_factory_list_begins_with_init() {
        let params = params();
        let all = factory(&params);
        assert_eq!(all[0].name, INIT_NAME);
        assert_eq!(all.len(), FACTORY_FILES.len() + 1);
    }

    #[test]
    fn no_factory_preset_moves_a_per_step_switch() {
        // Accent-note and slide are the sequencer's per-step buttons. A preset that shipped one on
        // would accent or slide **every** note the moment it loaded, which is not a sound — it is a
        // stuck button.
        for (name, text) in FACTORY_FILES {
            let preset = Preset::parse(text, crate::CLAP_ID).expect("parses");
            let v = |id: &str| preset.params.get(id).map_or(0.0, |value| value.v);
            assert!(v("accentnote") < 0.5, "{name:?} accents every note");
            assert!(v("slide") < 0.5, "{name:?} slides every note");
        }
    }
}

//! mxm-mono-03's routing parameters: one presence and one amount per *(target, source)* pair.
//!
//! `plans/plan-mxm-mono-03-modulation.md` §6, under `plans/plan-modulation-routing.md` §4.3. The
//! derive needs concrete fields and this instrument's source list is its own, so the struct is
//! declared here rather than generated — the shape `mxm-mono-01` and `mxm-poly-06` use. What is
//! shared is everything around these fields: [`mxm_modulation_params`] reads them, and
//! [`mxm_mono_03_dsp::routing`] evaluates them.
//!
//! # Permanent ids
//!
//! One `#[nested(id_prefix = …)]` per target, so a pair's ids are `<target>_<source>` and
//! `<target>_<source>on`. **Permanent from here on**, like every id in this collection.

use mxm_modulation_params::Route;
use mxm_modulation_params::reading::{self, Fader, Reach};
use mxm_mono_03_dsp::routing::{
    FULL_SCALE, INIT_PRESENT, KEY_UNIT_SEMITONES, Routing, SOURCE_NAMES, SOURCE_PEAK, SOURCES,
    TARGET_NAMES, TARGETS, init_amount, offer, source, target,
};
use nice_plug::prelude::*;

/// Every routing pair's two permanent ids, `(amount, presence)`, in `[target][source]` order.
///
/// **Written out rather than derived at runtime**, because a preset's parameter list is
/// `&'static str` and because these are permanent ids: they belong in the source where they can be
/// read, grepped and diffed. The `#[nested(id_prefix = …)]` groups in [`Routes`] still generate
/// them; `tests::the_id_table_is_what_the_derive_actually_produces` holds the two together.
pub const ROUTE_IDS: [[(&str, &str); SOURCES]; TARGETS] = [
    [
        ("mod_cutoff_key", "mod_cutoff_keyon"),
        ("mod_cutoff_vel", "mod_cutoff_velon"),
        ("mod_cutoff_wheel", "mod_cutoff_wheelon"),
        ("mod_cutoff_press", "mod_cutoff_presson"),
        ("mod_cutoff_bend", "mod_cutoff_bendon"),
        ("mod_cutoff_env", "mod_cutoff_envon"),
        ("mod_cutoff_accent", "mod_cutoff_accenton"),
        ("mod_cutoff_accentlevel", "mod_cutoff_accentlevelon"),
        ("mod_cutoff_osc", "mod_cutoff_oscon"),
    ],
    [
        ("mod_res_key", "mod_res_keyon"),
        ("mod_res_vel", "mod_res_velon"),
        ("mod_res_wheel", "mod_res_wheelon"),
        ("mod_res_press", "mod_res_presson"),
        ("mod_res_bend", "mod_res_bendon"),
        ("mod_res_env", "mod_res_envon"),
        ("mod_res_accent", "mod_res_accenton"),
        ("mod_res_accentlevel", "mod_res_accentlevelon"),
        ("mod_res_osc", "mod_res_oscon"),
    ],
    [
        ("mod_amp_key", "mod_amp_keyon"),
        ("mod_amp_vel", "mod_amp_velon"),
        ("mod_amp_wheel", "mod_amp_wheelon"),
        ("mod_amp_press", "mod_amp_presson"),
        ("mod_amp_bend", "mod_amp_bendon"),
        ("mod_amp_env", "mod_amp_envon"),
        ("mod_amp_accent", "mod_amp_accenton"),
        ("mod_amp_accentlevel", "mod_amp_accentlevelon"),
        ("mod_amp_osc", "mod_amp_oscon"),
    ],
];

/// One target's routes: a presence and a signed amount for every source the instrument declares.
///
/// **Presence is the enable and the amount is the depth**, and nothing else: no selector, because a
/// pair *is* its source; no polarity switch, because the amount is signed. Declared in source order.
#[derive(Params)]
pub struct TargetRoutes {
    #[id = "keyon"]
    pub key_on: BoolParam,
    #[id = "key"]
    pub key: FloatParam,
    #[id = "velon"]
    pub vel_on: BoolParam,
    #[id = "vel"]
    pub vel: FloatParam,
    #[id = "wheelon"]
    pub wheel_on: BoolParam,
    #[id = "wheel"]
    pub wheel: FloatParam,
    #[id = "presson"]
    pub press_on: BoolParam,
    #[id = "press"]
    pub press: FloatParam,
    #[id = "bendon"]
    pub bend_on: BoolParam,
    #[id = "bend"]
    pub bend: FloatParam,
    #[id = "envon"]
    pub env_on: BoolParam,
    #[id = "env"]
    pub env: FloatParam,
    #[id = "accenton"]
    pub accent_on: BoolParam,
    #[id = "accent"]
    pub accent: FloatParam,
    #[id = "accentlevelon"]
    pub accentlevel_on: BoolParam,
    #[id = "accentlevel"]
    pub accentlevel: FloatParam,
    #[id = "oscon"]
    pub osc_on: BoolParam,
    #[id = "osc"]
    pub osc: FloatParam,
}

/// A route amount: signed, and starting where the init patch puts it — **zero for every route but
/// the accent circuit's two**, which start at full because the Accent knob is their depth
/// (`mxm_mono_03_dsp::routing::INIT_AT_FULL`).
///
/// Smoothed at this instrument's own 10 ms, the smoothing its cutoff, resonance and accent have —
/// `plugins/AGENTS.md`'s *smooth signals, not coefficients*. It is the collection's one route
/// parameter (`mxm_modulation_params::reading`), on the travel the pair's offer allows — both halves,
/// on every pair here — and it reads as [`reach`] says.
fn amount(target: usize, source: usize) -> FloatParam {
    reading::amount_param_at(
        format!("{} from {}", TARGET_NAMES[target], SOURCE_NAMES[source]),
        init_amount(target, source),
        reach(target, source),
        Fader::for_offer(offer(target, source), false),
        10.0,
    )
}

/// What a route reads: **what its pair delivers at this amount with its source at its peak, in the
/// target's own unit** — octaves of cutoff, a percentage of the resonance control, a percentage of
/// the amplifier's level — and per octave of keyboard for a Key route
/// (`docs/code-review-notes.md` §7, *what a route's amount reads*). So the machine's own routes read
/// what they deliver at full: +4.60 oct of envelope above the circuit's floor, +6.40 oct of accent
/// sweep, and +100 % of the accent's level; a route the TB-303 never had reads the collection's
/// standard reach, +4.00 oct or +100 %.
fn reach(target: usize, source: usize) -> Reach {
    let unit = match target {
        target::CUTOFF => reading::OCTAVES,
        _ => reading::PERCENT,
    };
    let full = FULL_SCALE[target][source];
    if source == source::KEY {
        Reach::per_octave(full * 12.0 / KEY_UNIT_SEMITONES, unit)
    } else {
        Reach::new(full * SOURCE_PEAK[source], unit)
    }
}

/// Whether a route exists. **Configuration, not an amount**, so its default is the machine's own
/// wiring: Env Mod and the accent circuit's two outputs are present in the init patch, everything
/// else absent.
fn present(target: usize, source: usize) -> BoolParam {
    BoolParam::new(
        format!("{} from {} on", TARGET_NAMES[target], SOURCE_NAMES[source]),
        INIT_PRESENT.contains(&(target, source)),
    )
}

impl TargetRoutes {
    /// Every pair for one target, at the init patch.
    pub fn new(target: usize) -> Self {
        Self {
            key_on: present(target, 0),
            key: amount(target, 0),
            vel_on: present(target, 1),
            vel: amount(target, 1),
            wheel_on: present(target, 2),
            wheel: amount(target, 2),
            press_on: present(target, 3),
            press: amount(target, 3),
            bend_on: present(target, 4),
            bend: amount(target, 4),
            env_on: present(target, 5),
            env: amount(target, 5),
            accent_on: present(target, 6),
            accent: amount(target, 6),
            accentlevel_on: present(target, 7),
            accentlevel: amount(target, 7),
            osc_on: present(target, 8),
            osc: amount(target, 8),
        }
    }

    /// This target's routes in **declared source order**. `target` is its index, because each row's
    /// keyboard scope is its parameter's permanent id and those live in [`ROUTE_IDS`], keyed by
    /// target.
    pub fn routes(&self, target: usize) -> [Route<'_>; SOURCES] {
        let ids = ROUTE_IDS[target];
        let pairs: [(&BoolParam, &FloatParam); SOURCES] = [
            (&self.key_on, &self.key),
            (&self.vel_on, &self.vel),
            (&self.wheel_on, &self.wheel),
            (&self.press_on, &self.press),
            (&self.bend_on, &self.bend),
            (&self.env_on, &self.env),
            (&self.accent_on, &self.accent),
            (&self.accentlevel_on, &self.accentlevel),
            (&self.osc_on, &self.osc),
        ];
        std::array::from_fn(|s| Route {
            source: SOURCE_NAMES[s],
            present: pairs[s].0,
            amount: pairs[s].1,
            present_id: ids[s].1,
            amount_id: ids[s].0,
        })
    }

    /// Whether each of this target's routes exists. Read **once per interval**, never per sample.
    pub fn presences(&self, target: usize) -> [bool; SOURCES] {
        mxm_modulation_params::presences(&self.routes(target))
    }

    /// One source's amount parameter, by index, in declared source order — a `match` rather than an
    /// array of references, so a source nothing reads costs a branch and no pointer stores.
    #[inline]
    fn amount_param(&self, source: usize) -> &FloatParam {
        match source {
            0 => &self.key,
            1 => &self.vel,
            2 => &self.wheel,
            3 => &self.press,
            4 => &self.bend,
            5 => &self.env,
            6 => &self.accent,
            7 => &self.accentlevel,
            _ => &self.osc,
        }
    }

    /// Snaps a newly present route's smoother to its stored value.
    ///
    /// **An absent route's smoother is not advanced, so it must not be resumed either.** While the
    /// pair was absent nothing called `next()`, but the parameter stayed editable: a host automating
    /// it, or a preset load, moves the *target* and leaves the smoother wherever the last live sample
    /// left it. Resuming from there ramps the route in from a stale number over a span the host's
    /// buffers decide (`docs/code-review-notes.md` §7).
    pub fn arm(&self, newly_present: &[bool; SOURCES]) {
        for (source, &now) in newly_present.iter().enumerate() {
            if now {
                let param = self.amount_param(source);
                param.smoothed.reset(param.value());
            }
        }
    }
}

/// All three targets' routes.
#[derive(Params)]
pub struct Routes {
    #[nested(id_prefix = "mod_cutoff", group = "Modulation - Cutoff")]
    pub cutoff: TargetRoutes,
    #[nested(id_prefix = "mod_res", group = "Modulation - Resonance")]
    pub res: TargetRoutes,
    #[nested(id_prefix = "mod_amp", group = "Modulation - Amplitude")]
    pub amp: TargetRoutes,
}

impl Default for Routes {
    fn default() -> Self {
        Self::new()
    }
}

impl Routes {
    /// The init patch: **the machine's own three paths present** — Env Mod at zero, which is the
    /// circuit's floor, and the accent circuit's two outputs at full, which the Accent knob at zero
    /// keeps silent.
    pub fn new() -> Self {
        Self {
            cutoff: TargetRoutes::new(target::CUTOFF),
            res: TargetRoutes::new(target::RESONANCE),
            amp: TargetRoutes::new(target::AMPLITUDE),
        }
    }

    /// The three targets, in declared target order.
    pub fn each(&self) -> [&TargetRoutes; TARGETS] {
        [&self.cutoff, &self.res, &self.amp]
    }

    /// Which routes are live, for the whole instrument. Once per interval.
    pub fn topology(&self) -> Routing {
        let mut routing = Routing::new();
        for (index, (slot, group)) in routing.present.iter_mut().zip(self.each()).enumerate() {
            *slot = group.presences(index);
        }
        routing.compact();
        routing
    }

    /// The topology for this interval, with every **newly present** route's smoother snapped to its
    /// stored value. `previous` is the topology the last interval ran, which the caller keeps.
    pub fn topology_from(&self, previous: &Routing) -> Routing {
        let routing = self.topology();
        for (index, group) in self.each().into_iter().enumerate() {
            let mut newly = [false; SOURCES];
            for (slot, (&now, &before)) in newly.iter_mut().zip(
                routing.present[index]
                    .iter()
                    .zip(previous.present[index].iter()),
            ) {
                *slot = now && !before;
            }
            group.arm(&newly);
        }
        routing
    }

    /// Fills this sample's amounts into an already-topologised [`Routing`]: **each live route's
    /// smoother advanced once**, and an absent route's left exactly where the player put it.
    #[inline]
    pub fn advance(&self, routing: &mut Routing) {
        let targets = self.each();
        for i in 0..routing.live().len() {
            let (t, s) = routing.live()[i];
            routing.amounts[t as usize][s as usize] =
                targets[t as usize].amount_param(s as usize).smoothed.next();
        }
    }

    /// Every routing parameter, named by its permanent id, for the preset layer. **Presets carry
    /// routing**, presences included, or a sound would load with somebody else's routes still in it.
    pub fn parameters(&self) -> Vec<(&'static str, &dyn mxm_preset::ErasedParam)> {
        let mut out = Vec::with_capacity(TARGETS * SOURCES * 2);
        for (index, (group, ids)) in self.each().into_iter().zip(ROUTE_IDS).enumerate() {
            for (route, (amount, presence)) in group.routes(index).into_iter().zip(ids) {
                out.push((amount, route.amount));
                out.push((presence, route.present));
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nice_plug::params::Params;

    /// [`ROUTE_IDS`] names exactly what the derive produces, and nothing else.
    #[test]
    fn the_id_table_is_what_the_derive_actually_produces() {
        let params = crate::params::MxmMono03Params::default();
        let real: std::collections::BTreeSet<String> = params
            .param_map()
            .into_iter()
            .map(|(id, _, _)| id)
            .filter(|id| id.starts_with("mod_"))
            .collect();
        let named: std::collections::BTreeSet<String> = ROUTE_IDS
            .iter()
            .flatten()
            .flat_map(|(amount, presence)| [(*amount).to_owned(), (*presence).to_owned()])
            .collect();
        assert_eq!(named, real, "ROUTE_IDS has drifted from the derived ids");
        assert_eq!(named.len(), TARGETS * SOURCES * 2);
    }

    /// **Remove, edit while absent, re-add: the route arrives at the depth the player set.**
    #[test]
    fn a_re_added_route_arrives_at_its_stored_depth_rather_than_ramping_from_a_stale_one() {
        use nice_plug::params::InternalParamMut;

        let routes = Routes::new();
        assert!(!routes.res.osc_on.value(), "this pair starts absent");

        const RATE: f32 = 48_000.0;
        unsafe {
            routes.res.osc_on._internal_set_plain_value(true);
            routes.res.osc._internal_set_plain_value(0.9);
            routes.res.osc._internal_update_smoother(RATE, true);
        }
        let routing = routes.topology_from(&Routing::new());
        let mut amounts = routing;
        routes.advance(&mut amounts);
        assert_eq!(amounts.amounts[target::RESONANCE][source::OSCILLATOR], 0.9);

        unsafe {
            routes.res.osc_on._internal_set_plain_value(false);
        }
        let absent = routes.topology_from(&routing);
        unsafe {
            routes.res.osc._internal_set_plain_value(-0.4);
            routes.res.osc._internal_update_smoother(RATE, false);
        }
        assert!(
            routes.res.osc.smoothed.is_smoothing(),
            "the edit must leave the smoother mid-ramp, or this proves nothing"
        );

        unsafe {
            routes.res.osc_on._internal_set_plain_value(true);
        }
        let mut back = routes.topology_from(&absent);
        routes.advance(&mut back);
        assert_eq!(
            back.amounts[target::RESONANCE][source::OSCILLATOR],
            -0.4,
            "a re-added route must arrive at its stored depth"
        );
    }

    /// **A route reads what its pair delivers**, in the target's unit and against its source's peak —
    /// and a reading typed back in lands on the amount it came from. The one defect a player meets on
    /// the first knob they turn, and no audio assertion can see it.
    #[test]
    fn a_route_reads_what_its_pair_delivers_and_reads_back() {
        use mxm_preset::ErasedParam;
        let r = Routes::new();
        let cases: [(&FloatParam, f32, &str); 12] = [
            // The machine's own, at the circuit's reach.
            (&r.cutoff.env, 1.0, "+4.60 oct"),
            (&r.cutoff.env, 0.0, "-4.60 oct"),
            (&r.cutoff.env, 0.5, "+0.00 oct"),
            (&r.cutoff.accent, 1.0, "+6.40 oct"),
            (&r.amp.accentlevel, 1.0, "+100 %"),
            (&r.res.accent, 1.0, "+160 %"),
            // Added ones, at the collection's standard reach.
            (&r.cutoff.key, 1.0, "+1.00 oct/oct"),
            (&r.cutoff.osc, 1.0, "+4.00 oct"),
            (&r.cutoff.vel, 1.0, "+4.00 oct"),
            (&r.res.press, 1.0, "+100 %"),
            (&r.amp.vel, 1.0, "+100 %"),
            (&r.amp.key, 1.0, "+20 %/oct"),
        ];
        for (param, normalised, expected) in cases {
            let text = ErasedParam::format(param, normalised);
            assert_eq!(
                text,
                expected,
                "{} at {normalised}",
                ErasedParam::name(param)
            );
            let back = param
                .string_to_normalized_value(&text)
                .expect("the reading parses");
            assert!(
                (back - normalised).abs() < 1e-3,
                "{}: {text} read back as {back}",
                ErasedParam::name(param)
            );
        }
    }

    use mxm_plugin_test::routing_checks;

    /// **Every route parameter says what the DSP does** — the modulation standard's plugin half:
    /// each pair's travel is its offer's, its reading carries its target's unit and states what
    /// `mxm_mono_03_dsp::conformance` measures the voice's own graph delivering, and every reading
    /// survives the host's round trip.
    ///
    /// Falsified before trusted: with the cutoff reading's reach left at Env Mod's five octaves for
    /// Velocity, it names that pair.
    #[test]
    fn every_route_parameter_says_what_the_dsp_does() {
        let routes = Routes::new();
        let groups = routes.each();
        if let Err(failures) =
            routing_checks::amounts(&mxm_mono_03_dsp::conformance::Declared, |target, source| {
                Some(groups[target].amount_param(source))
            })
        {
            panic!("{} failure(s):\n{}", failures.len(), failures.join("\n"));
        }
    }

    /// **Every route's reading reads back as itself, amounts that round to zero included.** A value
    /// that rounded to zero printed `-0 %/oct`, which parses to zero and prints `+0 %/oct`: text that is
    /// not idempotent through a host's conversion, which `clap-validator`'s `param-conversions`
    /// rejects.
    #[test]
    fn every_reading_survives_the_hosts_round_trip_a_rounded_zero_included() {
        use mxm_preset::ErasedParam;
        let r = Routes::new();
        for group in r.each() {
            for s in 0..SOURCES {
                let param = group.amount_param(s);
                for normalised in [
                    0.0f32, 0.25, 0.4999, 0.49999, 0.5, 0.50001, 0.5001, 0.75, 1.0,
                ] {
                    let text = ErasedParam::format(param, normalised);
                    let back = param
                        .string_to_normalized_value(&text)
                        .expect("the reading parses");
                    assert_eq!(
                        text,
                        ErasedParam::format(param, back),
                        "{} at {normalised}",
                        ErasedParam::name(param)
                    );
                }
            }
        }
    }

    /// **A control-map role may take a route's amount only where Init wires that route**, or its
    /// knob is dead on a fresh instance. A role bound to a presence is never dead.
    #[test]
    fn a_control_map_role_never_points_at_a_dead_route() {
        let text = include_str!("../control-map.json");
        let params = crate::params::MxmMono03Params::default();
        let mut named = 0;
        for (t, group) in params.routes.each().into_iter().enumerate() {
            for (route, (amount, presence)) in group.routes(t).into_iter().zip(ROUTE_IDS[t]) {
                if text.contains(&format!("\"{presence}\"")) {
                    named += 1;
                }
                if text.contains(&format!("\"{amount}\"")) {
                    named += 1;
                    assert!(
                        route.is_present(),
                        "the control map binds a role to {amount}, which Init does not wire: a \
                         knob on it would do nothing"
                    );
                }
            }
        }
        assert_eq!(
            named, 1,
            "the map names {named} routing ids; it named one, `filter.env_amount`, when this was \
             written"
        );
    }
}

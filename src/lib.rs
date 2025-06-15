use nih_plug::prelude::*;
use std::sync::Arc;

mod condenser;
use condenser::Condenser;

const TH_DB: f32 = -40.0;
const DRY_WET: f32 = 0.5;
const FADE_MS: f32 = 10.0;
const REL_MS: f32 = 50.0;
const RING_SEC: i32 = 60;
const WARMUP_S: f32 = 0.3;
const LOOP_MODE: bool = false;

// This is a shortened version of the gain example with most comments removed, check out
// https://github.com/robbert-vdh/nih-plug/blob/master/plugins/examples/gain/src/lib.rs to get
// started

struct CondenserRs {
    params: Arc<CondenserRsParams>,
    fx_l: Option<Condenser>,
    fx_r: Option<Condenser>,
}

#[derive(Params)]
struct CondenserRsParams {
    #[id = "threshold_db"]
    pub threshold_db: FloatParam,

    #[id = "dry_wet"]
    pub dry_wet: FloatParam,

    #[id = "fade_ms"]
    pub fade_ms: FloatParam,

    #[id = "rel_ms"]
    pub rel_ms: FloatParam,

    #[id = "ring_sec"]
    pub ring_sec: IntParam,

    #[id = "warmup_s"]
    pub warmup_s: FloatParam,

    #[id = "loop_mode"]
    pub loop_mode: BoolParam,
}

impl Default for CondenserRs {
    fn default() -> Self {
        Self {
            params: Arc::new(CondenserRsParams::default()),
            fx_l: None,
            fx_r: None,
        }
    }
}

impl Default for CondenserRsParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                TH_DB,
                FloatRange::Linear {
                    min: -80.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB"),

            dry_wet: FloatParam::new(
                "Dry/Wet",
                DRY_WET,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            ),

            fade_ms: FloatParam::new(
                "Fade",
                FADE_MS,
                FloatRange::Linear {
                    min: 1.0,
                    max: 100.0,
                },
            )
            .with_unit(" ms"),

            rel_ms: FloatParam::new(
                "Release",
                REL_MS,
                FloatRange::Linear {
                    min: 1.0,
                    max: 1000.0,
                },
            )
            .with_unit(" ms"),

            ring_sec: IntParam::new(
                "Loop Length",
                RING_SEC,
                IntRange::Linear { min: 1, max: 120 },
            ),

            warmup_s: FloatParam::new(
                "Warmup",
                WARMUP_S,
                FloatRange::Linear { min: 0.0, max: 5.0 },
            )
            .with_unit(" s"),

            loop_mode: BoolParam::new("Loop Mode", LOOP_MODE),
        }
    }
}

impl Plugin for CondenserRs {
    const NAME: &'static str = "Condenser Rs";
    const VENDOR: &'static str = "Daishi Suzuki";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "zukky.rikugame@gmail.com";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // The first audio IO layout is used as the default. The other layouts may be selected either
    // explicitly or automatically by the host or the user depending on the plugin API/backend.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),

        aux_input_ports: &[],
        aux_output_ports: &[],

        // Individual ports and the layout as a whole can be named here. By default these names
        // are generated as needed. This layout will be called 'Stereo', while a layout with
        // only one input and output channel would be called 'Mono'.
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    // If the plugin can send or receive SysEx messages, it can define a type to wrap around those
    // messages here. The type implements the `SysExMessage` trait, which allows conversion to and
    // from plain byte buffers.
    type SysExMessage = ();
    // More advanced plugins can use this to run expensive background tasks. See the field's
    // documentation for more information. `()` means that the plugin does not have any background
    // tasks.
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        let fs = buffer_config.sample_rate as usize;
        let p = &self.params;
        self.fx_l = Some(Condenser::new(
            fs,
            p.threshold_db.value(),
            p.dry_wet.value(),
            p.fade_ms.value(),
            p.rel_ms.value(),
            p.ring_sec.value() as usize,
            p.warmup_s.value(),
            p.loop_mode.value(),
        ));
        self.fx_r = Some(Condenser::new(
            fs,
            p.threshold_db.value(),
            p.dry_wet.value(),
            p.fade_ms.value(),
            p.rel_ms.value(),
            p.ring_sec.value() as usize,
            p.warmup_s.value(),
            p.loop_mode.value(),
        ));
        true
    }

    fn reset(&mut self) {
        let p = &self.params;
        if let Some(fx) = &mut self.fx_l {
            *fx = Condenser::new(
                fx.fs,
                p.threshold_db.value(),
                p.dry_wet.value(),
                p.fade_ms.value(),
                p.rel_ms.value(),
                p.ring_sec.value() as usize,
                p.warmup_s.value(),
                p.loop_mode.value(),
            );
        }
        if let Some(fx) = &mut self.fx_r {
            *fx = Condenser::new(
                fx.fs,
                p.threshold_db.value(),
                p.dry_wet.value(),
                p.fade_ms.value(),
                p.rel_ms.value(),
                p.ring_sec.value() as usize,
                p.warmup_s.value(),
                p.loop_mode.value(),
            );
        }
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let channels = buffer.as_slice();
        let p = &self.params;
        if let Some(fx) = &mut self.fx_l {
            fx.set_threshold_db(p.threshold_db.value());
            fx.set_dry_wet(p.dry_wet.value());
            fx.set_fade_ms(p.fade_ms.value());
            fx.set_rel_ms(p.rel_ms.value());
            fx.set_ring_sec(p.ring_sec.value() as usize);
            fx.set_warmup_sec(p.warmup_s.value());
            fx.set_loop_mode(p.loop_mode.value());

            if let Some(ch) = channels.get_mut(0) {
                fx.process_inplace(*ch);
            }
        }
        if let Some(fx) = &mut self.fx_r {
            fx.set_threshold_db(p.threshold_db.value());
            fx.set_dry_wet(p.dry_wet.value());
            fx.set_fade_ms(p.fade_ms.value());
            fx.set_rel_ms(p.rel_ms.value());
            fx.set_ring_sec(p.ring_sec.value() as usize);
            fx.set_warmup_sec(p.warmup_s.value());
            fx.set_loop_mode(p.loop_mode.value());

            if let Some(ch) = channels.get_mut(1) {
                fx.process_inplace(*ch);
            }
        }
        ProcessStatus::Normal
    }
}

impl ClapPlugin for CondenserRs {
    const CLAP_ID: &'static str = "com.zukky.condenser-rs";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("condencer effect");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // Don't forget to change these features
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for CondenserRs {
    const VST3_CLASS_ID: [u8; 16] = *b"CondenserEffect!";

    // And also don't forget to change these categories
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(CondenserRs);
nih_export_vst3!(CondenserRs);

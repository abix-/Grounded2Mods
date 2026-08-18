//! Gameplay Settings tab: shows every field from
//! S_GameplaySettings on BP_SGKGameInstance_C. Doubles use
//! sliders; Bools use checkboxes. Scrollable.

use ueforge::ui;

use crate::gameplay::{self, FieldType, FIELDS};

struct CachedState {
    loaded: bool,
    doubles: Vec<f32>,
    bools: Vec<bool>,
}

static STATE: std::sync::OnceLock<std::sync::Mutex<CachedState>> =
    std::sync::OnceLock::new();

fn state() -> &'static std::sync::Mutex<CachedState> {
    STATE.get_or_init(|| {
        let n = FIELDS.len();
        std::sync::Mutex::new(CachedState {
            loaded: false,
            doubles: vec![0.0; n],
            bools: vec![false; n],
        })
    })
}

fn load_all(s: &mut CachedState) -> Result<(), String> {
    let acc = gameplay::accessor()?;
    for (i, field) in FIELDS.iter().enumerate() {
        let abs = acc.base_offset() + field.offset;
        match field.ty {
            FieldType::Double => {
                let v: f64 = unsafe { (acc.ptr().add(abs) as *const f64).read_unaligned() };
                s.doubles[i] = v as f32;
            }
            FieldType::Bool => {
                let v: u8 = unsafe { *acc.ptr().add(abs) };
                s.bools[i] = v != 0;
            }
        }
    }
    s.loaded = true;
    Ok(())
}

fn slider_range(name: &str, current: f32) -> (f32, f32) {
    let (lo, hi) = match name {
        "ShiningsTimer" => (0.0, 120.0),
        "DayLength" | "NightLength" => (0.0, 60.0),
        "WeatherCycleDuration" => (0.0, 120.0),
        "InitialSeason" => (0.0, 4.0),
        "RespawnHealthMultiplier" => (0.0, 2.0),
        _ => (0.0, 10.0),
    };
    let hi = if current > hi { current * 2.0 } else { hi };
    (lo, hi)
}

pub fn render() {
    ui::text("Gameplay settings");
    ui::text_disabled("Drag sliders or toggle checkboxes. Click Refresh after loading a save.");
    ui::spacing();

    let Ok(mut s) = state().lock() else { return };

    if ui::button("Refresh") {
        let _ = load_all(&mut s);
    }

    if !s.loaded {
        ui::text_disabled("Click Refresh after loading a save.");
        return;
    }

    ui::separator();
    ui::spacing();
    ui::begin_child("##gp_scroll", 0.0, 0.0);

    for (i, field) in FIELDS.iter().enumerate() {
        match field.ty {
            FieldType::Double => {
                let (lo, hi) = slider_range(field.name, s.doubles[i]);
                ui::text(field.name);
                ui::same_line();
                ui::text_disabled(field.desc);
                ui::set_next_item_width(250.0);
                let label = format!("##gp_{}", field.name);
                if ui::slider_f32(&label, &mut s.doubles[i], lo, hi) {
                    if let Ok(acc) = gameplay::accessor() { acc.write_double(field, s.doubles[i] as f64); }
                }
            }
            FieldType::Bool => {
                let label = format!("{}##gp_b", field.name);
                if ui::checkbox(&label, &mut s.bools[i]) {
                    if let Ok(acc) = gameplay::accessor() { acc.write_bool(field, s.bools[i]); }
                }
                ui::same_line();
                ui::text_disabled(field.desc);
            }
        }
        ui::spacing();
    }

    ui::dummy(0.0, 40.0);
    ui::end_child();
}

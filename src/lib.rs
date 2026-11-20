//! Shulkerbox-Tooltip: zeigt den Inhalt einer Shulkerbox als 9×3-Raster im Tooltip.
//!
//! - Nur solange die eingestellte Taste gedrückt ist (Standard: linke Strg-Taste);
//!   sonst bleibt der Tooltip wie in Vanilla. Die Taste stellt man in
//!   Optionen → Steuerung ein, im Mod-Menü wählt man, ob diese Taste, Strg, Shift, Alt
//!   oder „immer“ gilt.
//! - Dann verschwindet die Textliste der Items, nur der Name bleibt über dem Raster.
//! - Die Slots und der Rahmen des Tooltips sind in der Farbe der Shulkerbox eingefärbt.
//!
//! Der Inhalt steht in der Item-Komponente `minecraft:container`: eine Liste aus
//! `{slot: Int, item: {id, count, components}}`.

use mod_api::{
    self, ClientHost, Host, HostBlockStates, Hooks, ItemPart, ItemStack, ItemTooltipEvent, KeyConflictContext,
    KeyRegistration, Registrar, RegistrationContext,
    RenderTooltipTextureEvent, SettingSpec, Tag, TooltipComponentDrawEvent,
    TooltipComponentEvent,
};

/// Die Mod-ID aus `sunrise-mod.toml`.
const MOD_ID: &str = "shulker_tooltip";
/// Die Taste in Optionen → Steuerung.
const KEY_ID: &str = "show_contents";
/// Die Einstellung in Optionen → Mods → shulker tooltip → Config.
const TRIGGER: &str = "trigger";

/// Spalten und Zeilen einer Shulkerbox.
const COLUMNS: usize = 9;
const ROWS: usize = 3;
const SLOTS: usize = COLUMNS * ROWS;
/// Ein Slot ist 18 GUI-Pixel groß, das Item darin 16.
const SLOT: f32 = 18.0;
/// Abstand nach oben zum Namen darüber.
const TOP_PADDING: f32 = 2.0;

#[derive(Default)]
struct Mod;

#[mod_api::sunrise_mod]
impl Hooks for Mod {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        // Alle 17 Shulkerboxen (alle Farben) auf einmal, über den Vanilla-Tag.
        r.item_part_tag("minecraft:shulker_boxes", ShulkerPreview);

        // Eigene Taste in Optionen → Steuerung (unter dem Namen der Mod).
        // `Gui`: sie gilt nur in Menüs wie dem Inventar und stört so nicht das Sprinten.
        r.register_key_mapping(
            KeyRegistration::new(KEY_ID, "key.keyboard.left.control")
                .label("key.shulker_tooltip.show_contents")
                .context(KeyConflictContext::Gui),
        );

        // Auswahl im Mod-Menü: welche Taste das Raster zeigt.
        let _ = r.register_setting(
            SettingSpec::choice(
                TRIGGER,
                "shulker_tooltip.setting.trigger",
                [
                    "shulker_tooltip.trigger.keybind",
                    "shulker_tooltip.trigger.control",
                    "shulker_tooltip.trigger.shift",
                    "shulker_tooltip.trigger.alt",
                    "shulker_tooltip.trigger.always",
                ],
                0,
            )
            .tooltip("shulker_tooltip.setting.trigger.tooltip"),
        );
    }

    /// Der Rahmen des Tooltips in der Farbe der Box, etwas heller als die Slots:
    /// oben stärker aufgehellt, unten wie die Slots (Vanilla-Rahmen ist ein Verlauf).
    fn render_tooltip_texture(
        &self,
        host: &dyn ClientHost,
        event: &mut RenderTooltipTextureEvent<'_>,
    ) {
        let Some(stack) = event.stack else { return };
        let Some(color) = preview_color(host, stack) else { return };
        event.border_top = 0xFF00_0000 | lighten(lighten(color));
        event.border_bottom = 0xFF00_0000 | lighten(color);
    }

    /// Mit gedrückter Taste über einer gefüllten Shulkerbox: nur den Namen behalten,
    /// die Textliste der Items fällt weg (das Raster zeigt sie ja).
    fn item_tooltip(&self, host: &dyn ClientHost, event: &mut ItemTooltipEvent<'_>) {
        if preview_color(host, event.stack).is_some() {
            event.lines.truncate(1);
        }
    }
}

/// Das Part, das an jeder Shulkerbox hängt und das Raster zeichnet.
struct ShulkerPreview;

#[mod_api::sunrise_part]
impl ItemPart for ShulkerPreview {
    fn tooltip_component(
        &self,
        host: &dyn ClientHost,
        event: &mut TooltipComponentEvent<'_>,
    ) -> Option<[f32; 2]> {
        // Ohne Taste oder bei leerer Box: kein Bild, Vanilla-Tooltip.
        preview_color(host, event.stack).map(|_| grid_size())
    }

    fn tooltip_component_draw(
        &self,
        host: &dyn ClientHost,
        event: &mut TooltipComponentDrawEvent<'_>,
    ) {
        let slots = contents(host, event.stack);
        let host_ref: &dyn Host = host;
        let color = box_color(host_ref, event.stack).unwrap_or(DEFAULT_COLOR);
        let tint = 0xFF00_0000 | lighten(color);

        let [x0, y0] = event.origin;
        let y0 = y0 + TOP_PADDING;

        for (index, slot) in slots.iter().enumerate() {
            let [x, y] = slot_position(index, [x0, y0]);

            // Slot-Hintergrund in der Farbe der Box (Vanilla-Slot, eingefärbt).
            let drawn = event.draw.sprite(
                "container/slot",
                [0.0, 0.0, SLOT, SLOT],
                [x, y, SLOT, SLOT],
                Some(tint),
            );
            if !drawn {
                event.draw.fill([x, y, SLOT, SLOT], 0xFF00_0000 | darken(color));
                event
                    .draw
                    .fill([x + 1.0, y + 1.0, SLOT - 2.0, SLOT - 2.0], tint);
            }

            // Das Item mit Anzahl (die Anzahl nur ab 2, wie im Inventar).
            if let Some(stack) = slot {
                let count = stack.count();
                let label = (count > 1).then(|| count.to_string());
                event
                    .draw
                    .gui_draw_item_with(stack, [x + 1.0, y + 1.0], label.as_deref());
            }
        }
    }
}

/// Ob das Raster gerade gezeigt werden soll, je nach Einstellung im Mod-Menü.
fn show_pressed(host: &dyn ClientHost) -> bool {
    let modifiers = host.modifiers();
    match host.setting_choice(TRIGGER).unwrap_or(0) {
        1 => modifiers.control(),
        2 => modifiers.shift(),
        3 => modifiers.alt(),
        4 => true,
        // 0: die Taste aus Optionen → Steuerung.
        _ => {
            if host.is_down(MOD_ID, KEY_ID) {
                return true;
            }
            // Ist sie auf Strg, Shift oder Alt gelegt, lesen wir die Modifier direkt:
            // die gehen auch zuverlässig, während das Inventar offen ist.
            match host.mod_key(MOD_ID, KEY_ID) {
                Some((key, _)) => modifier_key_held(
                    &key,
                    modifiers.control(),
                    modifiers.shift(),
                    modifiers.alt(),
                ),
                None => false,
            }
        }
    }
}

/// Ob die Taste `key` (`key.keyboard.left.control`) eine gedrückte Modifier-Taste ist.
fn modifier_key_held(key: &str, control: bool, shift: bool, alt: bool) -> bool {
    (key.ends_with(".control") && control)
        || (key.ends_with(".shift") && shift)
        || (key.ends_with(".alt") && alt)
}

/// Die Farbe der Box, wenn gerade ihr Raster gezeigt wird: Taste gedrückt,
/// es ist eine Shulkerbox und sie ist nicht leer. Sonst `None`.
fn preview_color(host: &dyn ClientHost, stack: &ItemStack) -> Option<u32> {
    if !show_pressed(host) {
        return None;
    }
    let host_ref: &dyn Host = host;
    let color = box_color(host_ref, stack)?;
    (!is_empty(&contents(host, stack))).then_some(color)
}

/// Größe des Rasters in GUI-Pixeln (Breite, Höhe).
fn grid_size() -> [f32; 2] {
    [COLUMNS as f32 * SLOT, ROWS as f32 * SLOT + TOP_PADDING]
}

/// Obere linke Ecke von Slot `index` (0..27), zeilenweise von links nach rechts.
fn slot_position(index: usize, origin: [f32; 2]) -> [f32; 2] {
    let column = (index % COLUMNS) as f32;
    let row = (index / COLUMNS) as f32;
    [origin[0] + column * SLOT, origin[1] + row * SLOT]
}

fn is_empty(slots: &[Option<ItemStack>; SLOTS]) -> bool {
    slots.iter().all(Option::is_none)
}

/// Farbe der normalen (ungefärbten) Shulkerbox.
const DEFAULT_COLOR: u32 = 0x97_6997;

/// Die Farbe einer Shulkerbox als `0xRRGGBB`, `None` wenn es keine Shulkerbox ist.
/// Die Farbe kommt zur Laufzeit aus dem Spiel: die Kartenfarbe (`MapColor`) des Blocks,
/// den das Item setzt. Die Mod enthält keine eigene Farbtabelle.
fn box_color(host: &dyn Host, stack: &ItemStack) -> Option<u32> {
    if !stack.item().id(host).ends_with("shulker_box") {
        return None;
    }
    let runtime = (|| {
        let block = host.item_block(stack.item())?;
        let state = host.block_state_default(block)?;
        let facts = host.block_states();
        let id = facts.map_color(state)?;
        // Volle Helligkeit: Farb-ID * 4 + 2.
        let [r, g, b, a] = facts.map_color_rgba(id.checked_mul(4)?.checked_add(2)?)?;
        (a > 0).then(|| u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
    })();
    Some(runtime.unwrap_or(DEFAULT_COLOR))
}

/// Mischt eine Farbe zur Hälfte mit Weiß, damit die Items darauf gut lesbar bleiben.
fn lighten(rgb: u32) -> u32 {
    mix(rgb, 0xFF_FFFF)
}

/// Mischt eine Farbe zur Hälfte mit Schwarz (für den Slot-Rand).
fn darken(rgb: u32) -> u32 {
    mix(rgb, 0x00_0000)
}

fn mix(a: u32, b: u32) -> u32 {
    let channel = |shift: u32| (((a >> shift) & 0xFF) + ((b >> shift) & 0xFF)) / 2 << shift;
    channel(16) | channel(8) | channel(0)
}

/// Die 27 Slots der Box, aus der Komponente `minecraft:container` gelesen.
fn contents(host: &dyn ClientHost, stack: &ItemStack) -> [Option<ItemStack>; SLOTS] {
    let mut slots: [Option<ItemStack>; SLOTS] = Default::default();
    let host: &dyn Host = host;

    let Some(components) = stack.components() else {
        return slots;
    };
    let Some(Tag::List(entries)) = components.as_nbt().get(&"minecraft:container".into()) else {
        return slots;
    };

    for entry in entries {
        let Tag::Compound(entry) = entry else { continue };
        let Some(index) = entry.get(&"slot".into()).and_then(slot_index) else {
            continue;
        };
        let Some(Tag::Compound(item)) = entry.get(&"item".into()) else {
            continue;
        };
        if let Some(stack) = ItemStack::from_nbt(host, item) {
            slots[index] = Some(stack);
        }
    }
    slots
}

/// Ein `slot`-Wert aus dem NBT als Index 0..27.
fn slot_index(tag: &Tag) -> Option<usize> {
    let value = match tag {
        Tag::Byte(v) => i64::from(*v),
        Tag::Short(v) => i64::from(*v),
        Tag::Int(v) => i64::from(*v),
        Tag::Long(v) => *v,
        _ => return None,
    };
    usize::try_from(value).ok().filter(|i| *i < SLOTS)
}

// `cargo test` prüft die Logik ohne das Spiel.
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_go_row_by_row() {
        assert_eq!(slot_position(0, [10.0, 20.0]), [10.0, 20.0]);
        assert_eq!(slot_position(8, [0.0, 0.0]), [8.0 * SLOT, 0.0]);
        assert_eq!(slot_position(9, [0.0, 0.0]), [0.0, SLOT]);
        assert_eq!(slot_position(26, [0.0, 0.0]), [8.0 * SLOT, 2.0 * SLOT]);
    }

    #[test]
    fn grid_is_nine_by_three() {
        assert_eq!(grid_size(), [162.0, 56.0]);
    }

    #[test]
    fn slot_numbers_outside_the_box_are_ignored() {
        assert_eq!(slot_index(&Tag::Byte(0)), Some(0));
        assert_eq!(slot_index(&Tag::Int(26)), Some(26));
        assert_eq!(slot_index(&Tag::Int(27)), None);
        assert_eq!(slot_index(&Tag::Int(-1)), None);
    }

    #[test]
    fn modifier_bindings_follow_their_key() {
        assert!(modifier_key_held("key.keyboard.left.control", true, false, false));
        assert!(modifier_key_held("key.keyboard.right.shift", false, true, false));
        assert!(modifier_key_held("key.keyboard.left.alt", false, false, true));
        assert!(!modifier_key_held("key.keyboard.left.control", false, true, true));
        assert!(!modifier_key_held("key.keyboard.x", true, true, true));
    }

    #[test]
    fn mixing_halves_each_channel() {
        assert_eq!(lighten(0x00_0000), 0x7F_7F7F);
        assert_eq!(darken(0xFF_FFFF), 0x7F_7F7F);
        assert_eq!(lighten(0xFF_0000), 0xFF_7F7F);
    }
}

use openaction::*;

use std::collections::HashSet;

use crate::gfx;
use crate::plugin::{ART_CACHE, is_active};

pub const TILE_1X1_UUID: ActionUuid = "com.opendeck.spotify-controller.art-1x1";
pub const TILE_2X2_UUID: ActionUuid = "com.opendeck.spotify-controller.art-2x2";
pub const TILE_3X3_UUID: ActionUuid = "com.opendeck.spotify-controller.art-3x3";
pub const TILE_4X4_UUID: ActionUuid = "com.opendeck.spotify-controller.art-4x4";

pub const TILE_ACTIONS: &[(ActionUuid, u8)] = &[
    (TILE_1X1_UUID, 1),
    (TILE_2X2_UUID, 2),
    (TILE_3X3_UUID, 3),
    (TILE_4X4_UUID, 4),
];

/// Result of checking whether all instances of an NxN tile action on one
/// device form a complete, contiguous NxN block.
struct TileLayout {
    valid: bool,
    anchor_row: u8,
    anchor_col: u8,
}

async fn analyze_grid(uuid: ActionUuid, device_id: &str, n: u8) -> TileLayout {
    let expected = (n as usize) * (n as usize);
    let instances: Vec<_> = visible_instances(uuid)
        .await
        .into_iter()
        .filter(|i| i.device_id == device_id && i.coordinates.is_some())
        .collect();

    let invalid = TileLayout { valid: false, anchor_row: 0, anchor_col: 0 };

    if instances.len() != expected {
        return invalid;
    }

    let coords: Vec<_> = instances
        .iter()
        .filter_map(|i| i.coordinates)
        .collect();

    let min_row = coords.iter().map(|c| c.row).min().unwrap();
    let min_col = coords.iter().map(|c| c.column).min().unwrap();
    let max_row = coords.iter().map(|c| c.row).max().unwrap();
    let max_col = coords.iter().map(|c| c.column).max().unwrap();

    if max_row - min_row + 1 != n || max_col - min_col + 1 != n {
        return invalid;
    }

    let mut seen: HashSet<(u8, u8)> = HashSet::new();
    for c in &coords {
        if !seen.insert((c.row, c.column)) {
            return invalid;
        }
    }

    TileLayout { valid: true, anchor_row: min_row, anchor_col: min_col }
}

async fn render_instance(instance: &Instance, uuid: ActionUuid, n: u8) {
    if !is_active() {
        if let Ok(uri) = gfx::inactive_tile() {
            let _ = instance.set_image(Some(uri), None).await;
        }
        return;
    }

    let Some(coords) = instance.coordinates else {
        return;
    };

    let layout = analyze_grid(uuid, &instance.device_id, n).await;
    if !layout.valid {
        if let Ok(uri) = gfx::misplaced_tile(n) {
            let _ = instance.set_image(Some(uri), None).await;
        }
        return;
    }

    let tr = coords.row - layout.anchor_row;
    let tc = coords.column - layout.anchor_col;

    let art_data = ART_CACHE.lock().await.data.clone();
    let icon = match art_data {
        Some(bytes) => gfx::render_art_tile(&bytes, n, tr, tc),
        None => gfx::inactive_tile(),
    };
    if let Ok(uri) = icon {
        let _ = instance.set_image(Some(uri), None).await;
    }
}

/// Repaint every instance of `uuid` on `device_id`. Used when the grid's
/// validity may have changed (instance added/removed).
pub async fn repaint_device(uuid: ActionUuid, n: u8, device_id: &str) {
    for inst in visible_instances(uuid).await {
        if inst.device_id == device_id {
            render_instance(&inst, uuid, n).await;
        }
    }
}

/// Repaint every instance of every tile action on every device. Used when the
/// album art or Spotify-running state changes.
pub async fn repaint_all() {
    for (uuid, n) in TILE_ACTIONS {
        for inst in visible_instances(uuid).await {
            render_instance(&inst, uuid, *n).await;
        }
    }
}

//! Plan variable-length mute bars without combining audio identities.
use super::*;
use crate::ui::overlay::group::{ClusterPeer, ClusterRequest, Side, GROUP_EXTRA};
use windows::Win32::Foundation::POINT;

fn eligible(entry: &LayoutEntry) -> bool {
    entry.key.is_compact_mute()
        && entry.lifetime.is_permanent()
        && (entry.compact || entry.collapse_started.is_some())
        && entry.model.rows.len() == 1
        && entry.model.rows[0].tone == super::super::model::OverlayTone::Muted
}

pub(super) fn choose_compact_groups(
    entries: &[LayoutEntry],
    previous: &[CompactGroup],
    now: Instant,
) -> Vec<CompactGroup> {
    let mut candidates = entries
        .iter()
        .filter(|entry| eligible(entry))
        .collect::<Vec<_>>();
    candidates.sort_by_key(|entry| {
        (
            !entry.compact,
            entry.compacted_at.unwrap_or(entry.presented_at),
            entry.sequence,
        )
    });
    let mut groups = Vec::new();
    while let Some(index) = candidates.iter().position(|entry| entry.compact) {
        let primary = candidates.remove(index);
        let side = Side::for_position(primary.placement.key.position);
        let mut peers = take_surface_peers(&mut candidates, primary);
        // Refreshing labels/generations changes registry sequence. Preserve
        // settled slots and append newcomers rather than shuffling the bar.
        peers.sort_by_key(|peer| {
            let slot = previous
                .iter()
                .position(|group| group.primary == primary.id && group.peer == peer.id);
            (
                slot.is_none(),
                slot.unwrap_or(usize::MAX),
                peer.collapse_started.unwrap_or(peer.presented_at),
                peer.id,
            )
        });
        for peer in peers {
            let started = previous
                .iter()
                .find(|group| {
                    group.primary == primary.id && group.peer == peer.id && group.side == side
                })
                .map_or_else(
                    || peer.collapse_started.unwrap_or(now),
                    |group| group.started,
                );
            groups.push(CompactGroup {
                primary: primary.id,
                peer: peer.id,
                side,
                started,
            });
        }
    }
    groups
}

fn take_surface_peers<'a>(
    candidates: &mut Vec<&'a LayoutEntry>,
    primary: &LayoutEntry,
) -> Vec<&'a LayoutEntry> {
    let factor =
        primary.render_config.scale.clamp(0.7, 1.6) * primary.placement.dpi.max(96) as f32 / 96.0;
    // Leave room at both edges. Overflow continues in another bar/lane.
    let available =
        (primary.placement.work.right - primary.placement.work.left) as f32 - 44.0 * factor;
    let capacity = ((available / factor - 52.0) / GROUP_EXTRA).floor().max(0.0) as usize;
    let mut peers = Vec::new();
    let mut index = 0;
    while index < candidates.len() && peers.len() < capacity {
        if same_group_surface(primary, candidates[index]) {
            peers.push(candidates.remove(index));
        } else {
            index += 1;
        }
    }
    peers.sort_by_key(|entry| entry.sequence);
    peers
}

pub(super) fn prepare_groups(
    entries: &mut Vec<LayoutEntry>,
    groups: &[CompactGroup],
) -> Vec<LayoutEntry> {
    let mut peers = Vec::new();
    for group in groups {
        let Some(index) = entries.iter().position(|entry| entry.id == group.peer) else {
            continue;
        };
        peers.push(entries.remove(index));
    }
    for primary in entries.iter_mut() {
        let members = groups
            .iter()
            .filter(|group| group.primary == primary.id)
            .collect::<Vec<_>>();
        let Some(first) = members.first() else {
            continue;
        };
        let cluster_peers = members
            .iter()
            .enumerate()
            .filter_map(|(index, group)| {
                let peer = peers.iter().find(|peer| peer.id == group.peer)?;
                Some(ClusterPeer {
                    id: peer.id,
                    row: peer.model.rows[0].clone(),
                    started: group.started,
                    offset: (index + 1) as f32
                        * GROUP_EXTRA
                        * if first.side == Side::Left { -1.0 } else { 1.0 },
                })
            })
            .collect::<Vec<_>>();
        let count = cluster_peers.len();
        primary.cluster = Some(ClusterRequest {
            peers: cluster_peers,
            side: first.side,
            started: members.iter().map(|group| group.started).max().unwrap(),
        });
        primary.size.cx = ((52.0 + GROUP_EXTRA * count as f32)
            * primary.render_config.scale.clamp(0.7, 1.6)
            * primary.placement.dpi.max(96) as f32
            / 96.0)
            .round() as i32;
    }
    peers
}

pub(super) fn group_peer_position(host: &PlannedEntry, peer: u64) -> POINT {
    let cluster = host.entry.cluster.as_ref().unwrap();
    let offset = cluster
        .peers
        .iter()
        .find(|entry| entry.id == peer)
        .unwrap()
        .offset;
    let factor = host.entry.render_config.scale.clamp(0.7, 1.6)
        * host.entry.placement.dpi.max(96) as f32
        / 96.0;
    let single = model_geometry(host.entry.render_config.scale, &host.entry.model, 1.0)
        .pixel_size(host.entry.placement.dpi);
    let primary_x = if cluster.side == Side::Left {
        host.entry.size.cx - single.cx
    } else {
        0
    };
    POINT {
        x: host.card.position.x + primary_x + (offset * factor).round() as i32,
        y: host.card.position.y,
    }
}

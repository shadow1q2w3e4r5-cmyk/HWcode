//! Interactive Hardware Timeline Visualizer & Chrome Trace / Perfetto Profiler (Phase 2).
//! Generates standalone interactive HTML/SVG timelines and standard Chrome Trace JSON.

use crate::simulator::SimulationResult;

pub fn generate_chrome_trace_json(sim: &SimulationResult) -> String {
    let mut entries = Vec::new();
    for ev in &sim.timeline {
        // Trace event format (Ph: "X" for Complete event)
        let name = ev.action.replace('\"', "\\\"");
        let cat = ev.resource.replace('\"', "\\\"");
        let ts_us = (ev.start_ns as f64) / 1000.0;
        let dur_us = ((ev.end_ns.saturating_sub(ev.start_ns)) as f64) / 1000.0;
        entries.push(format!(
            r#"{{"name":"{}","cat":"{}","ph":"X","ts":{:.3},"dur":{:.3},"pid":1,"tid":"{}"}}"#,
            name, cat, ts_us, dur_us, ev.resource
        ));
    }
    format!("[\n  {}\n]", entries.join(",\n  "))
}

pub fn generate_interactive_html(sim: &SimulationResult, title: &str) -> String {
    let total_us = (sim.total_time_ns as f64) / 1000.0;
    let max_time = if sim.total_time_ns == 0 { 1 } else { sim.total_time_ns };

    // Group events by resource track
    let mut tracks: Vec<String> = Vec::new();
    for ev in &sim.timeline {
        if !tracks.contains(&ev.resource) {
            tracks.push(ev.resource.clone());
        }
    }

    let mut event_cards_html = Vec::new();
    for ev in &sim.timeline {
        let left_pct = ((ev.start_ns as f64) / (max_time as f64)) * 100.0;
        let dur_ns = ev.end_ns.saturating_sub(ev.start_ns);
        let width_pct = ((dur_ns as f64) / (max_time as f64) * 100.0).max(0.6);
        let track_idx = tracks.iter().position(|r| r == &ev.resource).unwrap_or(0);
        let top_px = track_idx * 46 + 40;

        let bg_color = if ev.action.starts_with("Kernel") {
            "#8b5cf6" // purple for GPU kernel
        } else if ev.action.starts_with("DMA") {
            "#3b82f6" // blue for DMA
        } else if ev.action.starts_with("LayoutConvert") {
            "#f59e0b" // amber for layout conversion
        } else if ev.action.starts_with("Route") || ev.action.starts_with("Fallback") {
            "#10b981" // green for routing
        } else if ev.action.contains("Contention") {
            "#ef4444" // red for contention
        } else if ev.action.contains("Barrier") {
            "#64748b" // slate for barrier
        } else {
            "#06b6d4" // cyan for others
        };

        event_cards_html.push(format!(
            r#"<div class="event-bar" style="left: {:.2}%; width: {:.2}%; top: {}px; background: {};" title="{}: {} ({} ns)">
                <span class="event-text">{}</span>
            </div>"#,
            left_pct, width_pct, top_px, bg_color, ev.resource, ev.action, dur_ns, ev.action
        ));
    }

    let track_labels = tracks
        .iter()
        .enumerate()
        .map(|(idx, name)| {
            format!(
                r#"<div class="track-label" style="top: {}px;">{}</div>"#,
                idx * 46 + 40,
                name
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let total_height = tracks.len() * 46 + 80;

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>HWCode Visual Hardware Timeline — {title}</title>
<style>
  body {{
    font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, monospace;
    background: #0f172a;
    color: #f8fafc;
    margin: 0;
    padding: 24px;
  }}
  h1 {{ margin: 0 0 8px 0; font-size: 22px; color: #38bdf8; }}
  .meta-bar {{
    display: flex;
    gap: 24px;
    margin-bottom: 24px;
    background: #1e293b;
    padding: 14px 20px;
    border-radius: 8px;
    font-size: 14px;
    border: 1px solid #334155;
  }}
  .meta-val {{ font-weight: bold; color: #f1f5f9; }}
  .timeline-container {{
    position: relative;
    background: #1e293b;
    border-radius: 8px;
    border: 1px solid #334155;
    height: {total_height}px;
    overflow-x: auto;
  }}
  .track-label {{
    position: absolute;
    left: 12px;
    width: 160px;
    font-size: 13px;
    font-weight: 600;
    color: #94a3b8;
    height: 38px;
    line-height: 38px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  .track-lanes {{
    position: absolute;
    left: 180px;
    right: 20px;
    height: 100%;
  }}
  .event-bar {{
    position: absolute;
    height: 34px;
    border-radius: 6px;
    color: #ffffff;
    cursor: pointer;
    font-size: 12px;
    padding: 0 8px;
    box-sizing: border-box;
    display: flex;
    align-items: center;
    overflow: hidden;
    white-space: nowrap;
    box-shadow: 0 2px 4px rgba(0,0,0,0.3);
    transition: transform 0.1s ease;
  }}
  .event-bar:hover {{
    transform: scaleY(1.15);
    z-index: 10;
    box-shadow: 0 4px 12px rgba(0,0,0,0.5);
  }}
  .event-text {{
    overflow: hidden;
    text-overflow: ellipsis;
  }}
  .legend {{
    margin-top: 18px;
    display: flex;
    gap: 16px;
    font-size: 13px;
  }}
  .legend-item {{ display: flex; align-items: center; gap: 6px; }}
  .legend-box {{ width: 14px; height: 14px; border-radius: 3px; }}
</style>
</head>
<body>
  <h1>HWCode Physical Hardware Timeline — {title}</h1>
  <div class="meta-bar">
    <div>Total Time: <span class="meta-val">{total_us:.3} us ({sim_time_ns} ns)</span></div>
    <div>DMA Transferred: <span class="meta-val">{bytes} bytes</span></div>
    <div>Peak Power: <span class="meta-val">{power:.1} W</span></div>
    <div>Junction Temp: <span class="meta-val">{temp:.1} &deg;C</span></div>
    <div>Status: <span class="meta-val" style="color: #4ade80;">{status}</span></div>
  </div>

  <div class="timeline-container">
    {track_labels}
    <div class="track-lanes">
      {event_cards}
    </div>
  </div>

  <div class="legend">
    <div class="legend-item"><div class="legend-box" style="background:#8b5cf6;"></div> GPU Kernel</div>
    <div class="legend-item"><div class="legend-box" style="background:#3b82f6;"></div> DMA Engine</div>
    <div class="legend-item"><div class="legend-box" style="background:#10b981;"></div> Route / Fallback</div>
    <div class="legend-item"><div class="legend-box" style="background:#f59e0b;"></div> Layout Convert</div>
    <div class="legend-item"><div class="legend-box" style="background:#ef4444;"></div> Contention Delay</div>
    <div class="legend-item"><div class="legend-box" style="background:#64748b;"></div> Memory Barrier</div>
  </div>
</body>
</html>"#,
        title = title,
        total_us = total_us,
        sim_time_ns = sim.total_time_ns,
        bytes = sim.total_bytes_transferred,
        power = sim.peak_power_watts,
        temp = sim.simulated_temp_c,
        status = sim.status,
        total_height = total_height,
        track_labels = track_labels,
        event_cards = event_cards_html.join("\n      ")
    )
}

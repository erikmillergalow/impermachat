use std::time::Duration;
use axum::http::HeaderMap;

pub fn name_to_color(name: &str) -> String {
    let mut hash: u32 = 0;
    for byte in name.bytes() {
        hash = hash.wrapping_add(byte as u32);
        hash = hash.wrapping_mul(31);
    }

    let r = (hash % 200) + 55; // +55 to avoid going too dark
    let g = ((hash >> 8) % 200) + 55;
    let b = ((hash >> 16) % 200) + 55;

    format!("#{:02x}{:02x}{:02x}", r, g, b)
}

pub fn format_time(remaining: Duration) -> String {
    let total_seconds = remaining.as_secs();
    let hours = total_seconds / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    format!("{:02}:{:02}:{:02} remaining...", hours, minutes, seconds)

}

pub fn get_connection_cookie(headers: &HeaderMap) -> Option<String> {
    headers.get("cookie")
        .and_then(|c| c.to_str().ok())
        .and_then(|c| c.split(';')
            .find(|s| s.trim().starts_with("impermachat_id="))
            .map(|s| s.trim_start_matches("impermachat_id=").to_string()))
}

pub fn create_fragments_event(rendered_template: String) -> String {
    let mut raw_event = String::from("");
    for line in rendered_template.lines() {
        raw_event.push_str(&format!("fragments {}\n", line));
    }
    raw_event
}

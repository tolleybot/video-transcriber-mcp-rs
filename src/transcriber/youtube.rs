use anyhow::{Context, Result};
use regex::Regex;
use std::sync::LazyLock;
use tracing::info;

use super::types::VideoMetadata;

static VIDEO_ID_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:youtube\.com/(?:watch\?.*v=|embed/|v/|shorts/)|youtu\.be/)([a-zA-Z0-9_-]{11})")
        .unwrap()
});

static INNERTUBE_API_KEY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#""INNERTUBE_API_KEY":\s*"([a-zA-Z0-9_-]+)""#).unwrap());

pub struct YouTubeTranscriptFetcher {
    client: reqwest::Client,
}

pub struct YouTubeTranscriptResult {
    pub transcript: String,
    pub metadata: VideoMetadata,
    pub language: String,
    pub is_auto_generated: bool,
}

impl Default for YouTubeTranscriptFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl YouTubeTranscriptFetcher {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .expect("Failed to create HTTP client");
        Self { client }
    }

    pub async fn fetch_transcript(
        &self,
        video_id: &str,
        preferred_language: Option<&str>,
    ) -> Result<YouTubeTranscriptResult> {
        let watch_url = format!("https://www.youtube.com/watch?v={}", video_id);

        // Step 1: Fetch the watch page to extract the InnerTube API key
        info!("Fetching YouTube page to extract API key...");
        let html = self
            .client
            .get(&watch_url)
            .header("Accept-Language", "en-US")
            .send()
            .await
            .context("Failed to fetch YouTube page")?
            .error_for_status()
            .context("YouTube returned an error status")?
            .text()
            .await
            .context("Failed to read YouTube page body")?;

        let api_key = INNERTUBE_API_KEY_RE
            .captures(&html)
            .map(|caps| caps[1].to_string())
            .context("Could not find INNERTUBE_API_KEY in YouTube page")?;

        // Step 2: Call the InnerTube player API (as Android client to avoid bot detection)
        info!("Calling InnerTube API for video data...");
        let innertube_url = format!("https://www.youtube.com/youtubei/v1/player?key={}", api_key);
        let innertube_body = serde_json::json!({
            "context": {
                "client": {
                    "clientName": "ANDROID",
                    "clientVersion": "20.10.38"
                }
            },
            "videoId": video_id
        });

        let player_response: serde_json::Value = self
            .client
            .post(&innertube_url)
            .json(&innertube_body)
            .send()
            .await
            .context("Failed to call InnerTube API")?
            .error_for_status()
            .context("InnerTube API returned an error status")?
            .json()
            .await
            .context("Failed to parse InnerTube API response")?;

        // Extract metadata from videoDetails
        let video_details = &player_response["videoDetails"];
        let metadata = VideoMetadata {
            video_id: video_id.to_string(),
            title: video_details["title"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            channel: video_details["author"]
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            duration: video_details["lengthSeconds"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            upload_date: player_response["microformat"]["playerMicroformatRenderer"]["publishDate"]
                .as_str()
                .unwrap_or("")
                .to_string(),
            platform: "YouTube".to_string(),
            url: watch_url,
        };

        // Extract caption tracks
        let caption_tracks =
            player_response["captions"]["playerCaptionsTracklistRenderer"]["captionTracks"]
                .as_array()
                .context("No caption tracks available for this video")?;

        if caption_tracks.is_empty() {
            anyhow::bail!("No caption tracks available for this video");
        }

        // Select best caption track
        let lang = preferred_language.unwrap_or("en");
        let (raw_track_url, track_lang, is_asr) = select_caption_track(caption_tracks, lang)
            .context("No suitable caption track found")?;

        // Strip fmt=srv3 to get simple <text> XML format instead of <p>/<s> format
        let track_url = raw_track_url.replace("&fmt=srv3", "");

        info!(
            "Found {} captions (language: {})",
            if is_asr { "auto-generated" } else { "manual" },
            track_lang
        );

        // Step 3: Fetch and parse the caption XML
        let caption_xml = self
            .client
            .get(&track_url)
            .send()
            .await
            .context("Failed to fetch caption XML")?
            .error_for_status()
            .context("Caption XML request returned an error status")?
            .text()
            .await
            .context("Failed to read caption XML")?;

        let transcript = parse_caption_xml(&caption_xml)?;

        if transcript.is_empty() {
            anyhow::bail!("Caption track returned empty transcript");
        }

        Ok(YouTubeTranscriptResult {
            transcript,
            metadata,
            language: track_lang,
            is_auto_generated: is_asr,
        })
    }
}

/// Extract video ID from a YouTube URL. Returns None for non-YouTube URLs.
pub fn extract_youtube_video_id(url: &str) -> Option<String> {
    VIDEO_ID_RE.captures(url).map(|caps| caps[1].to_string())
}

/// Select the best caption track based on language preference.
/// Returns (url, language_code, is_auto_generated).
fn select_caption_track(
    tracks: &[serde_json::Value],
    preferred_lang: &str,
) -> Option<(String, String, bool)> {
    let mut manual_match: Option<(String, String)> = None;
    let mut asr_match: Option<(String, String)> = None;
    let mut first_manual: Option<(String, String)> = None;
    let mut first_asr: Option<(String, String)> = None;

    for track in tracks {
        let Some(base_url) = track["baseUrl"].as_str() else {
            continue;
        };
        let lang_code = track["languageCode"].as_str().unwrap_or("");
        let is_asr = track["kind"].as_str() == Some("asr");

        if lang_code == preferred_lang {
            if is_asr {
                asr_match = Some((base_url.to_string(), lang_code.to_string()));
            } else {
                manual_match = Some((base_url.to_string(), lang_code.to_string()));
            }
        }

        if !is_asr && first_manual.is_none() {
            first_manual = Some((base_url.to_string(), lang_code.to_string()));
        }
        if is_asr && first_asr.is_none() {
            first_asr = Some((base_url.to_string(), lang_code.to_string()));
        }
    }

    // Priority: manual match > ASR match > first manual > first ASR
    if let Some((url, lang)) = manual_match {
        Some((url, lang, false))
    } else if let Some((url, lang)) = asr_match {
        Some((url, lang, true))
    } else if let Some((url, lang)) = first_manual {
        Some((url, lang, false))
    } else {
        first_asr.map(|(url, lang)| (url, lang, true))
    }
}

/// Parse YouTube caption XML and return the transcript text.
fn parse_caption_xml(xml: &str) -> Result<String> {
    let mut segments = Vec::new();

    for piece in xml.split("<text ") {
        // Find the text content between > and </text>
        if let Some(start) = piece.find('>') {
            let content = &piece[start + 1..];
            if let Some(end) = content.find("</text>") {
                let text = decode_html_entities(&content[..end]);
                let text = text.trim();
                if !text.is_empty() {
                    segments.push(text.to_string());
                }
            }
        }
    }

    Ok(segments.join(" "))
}

/// Decode HTML entities in a single pass to avoid double-decode issues
/// (e.g. `&amp;lt;` should become `&lt;`, not `<`).
fn decode_html_entities(s: &str) -> String {
    static ENTITY_RE: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"&(amp|lt|gt|quot|apos|nbsp|#(\d+)|#x([0-9a-fA-F]+));").unwrap()
    });

    let decoded = ENTITY_RE
        .replace_all(s, |caps: &regex::Captures| {
            match &caps[1] {
                "amp" => "&".to_string(),
                "lt" => "<".to_string(),
                "gt" => ">".to_string(),
                "quot" => "\"".to_string(),
                "apos" => "'".to_string(),
                "nbsp" => " ".to_string(),
                _ if caps.get(2).is_some() => {
                    // Decimal numeric reference: &#123;
                    caps[2]
                        .parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| caps[0].to_string())
                }
                _ if caps.get(3).is_some() => {
                    // Hex numeric reference: &#x7B;
                    u32::from_str_radix(&caps[3], 16)
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                        .unwrap_or_else(|| caps[0].to_string())
                }
                _ => caps[0].to_string(),
            }
        })
        .to_string();

    decoded.replace('\n', " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── extract_youtube_video_id ──────────────────────────────────────

    #[test]
    fn test_extract_id_standard_url() {
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        assert_eq!(
            extract_youtube_video_id(url).as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn test_extract_id_short_url() {
        let url = "https://youtu.be/dQw4w9WgXcQ";
        assert_eq!(
            extract_youtube_video_id(url).as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn test_extract_id_embed_url() {
        let url = "https://www.youtube.com/embed/dQw4w9WgXcQ";
        assert_eq!(
            extract_youtube_video_id(url).as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn test_extract_id_shorts_url() {
        let url = "https://www.youtube.com/shorts/dQw4w9WgXcQ";
        assert_eq!(
            extract_youtube_video_id(url).as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn test_extract_id_with_extra_params() {
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=120&list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf";
        assert_eq!(
            extract_youtube_video_id(url).as_deref(),
            Some("dQw4w9WgXcQ")
        );
    }

    #[test]
    fn test_extract_id_non_youtube_url() {
        assert!(extract_youtube_video_id("https://vimeo.com/123456").is_none());
    }

    #[test]
    fn test_extract_id_plain_text() {
        assert!(extract_youtube_video_id("not a url at all").is_none());
    }

    // ── extract_player_response ──────────────────────────────────────

    // ── extract innertube api key ──────────────────────────────────────

    #[test]
    fn test_extract_innertube_api_key() {
        let html = r#"some stuff "INNERTUBE_API_KEY": "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8" more stuff"#;
        let key = INNERTUBE_API_KEY_RE
            .captures(html)
            .map(|caps| caps[1].to_string());
        assert_eq!(
            key.as_deref(),
            Some("AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8")
        );
    }

    #[test]
    fn test_extract_innertube_api_key_missing() {
        let html = "<html>no api key here</html>";
        assert!(INNERTUBE_API_KEY_RE.captures(html).is_none());
    }

    // ── select_caption_track ─────────────────────────────────────────

    fn make_track(lang: &str, kind: Option<&str>, url: &str) -> serde_json::Value {
        let mut track = json!({
            "baseUrl": url,
            "languageCode": lang,
        });
        if let Some(k) = kind {
            track["kind"] = json!(k);
        }
        track
    }

    #[test]
    fn test_select_prefers_manual_match() {
        let tracks = vec![
            make_track("en", Some("asr"), "http://asr-en"),
            make_track("en", None, "http://manual-en"),
        ];
        let (url, lang, is_asr) = select_caption_track(&tracks, "en").unwrap();
        assert_eq!(url, "http://manual-en");
        assert_eq!(lang, "en");
        assert!(!is_asr);
    }

    #[test]
    fn test_select_falls_back_to_asr_match() {
        let tracks = vec![
            make_track("en", Some("asr"), "http://asr-en"),
            make_track("fr", None, "http://manual-fr"),
        ];
        let (url, _, is_asr) = select_caption_track(&tracks, "en").unwrap();
        assert_eq!(url, "http://asr-en");
        assert!(is_asr);
    }

    #[test]
    fn test_select_falls_back_to_first_manual() {
        let tracks = vec![
            make_track("es", None, "http://manual-es"),
            make_track("fr", Some("asr"), "http://asr-fr"),
        ];
        let (url, lang, is_asr) = select_caption_track(&tracks, "de").unwrap();
        assert_eq!(url, "http://manual-es");
        assert_eq!(lang, "es");
        assert!(!is_asr);
    }

    #[test]
    fn test_select_falls_back_to_first_asr() {
        let tracks = vec![make_track("ja", Some("asr"), "http://asr-ja")];
        let (url, lang, is_asr) = select_caption_track(&tracks, "en").unwrap();
        assert_eq!(url, "http://asr-ja");
        assert_eq!(lang, "ja");
        assert!(is_asr);
    }

    #[test]
    fn test_select_empty_tracks() {
        let tracks: Vec<serde_json::Value> = vec![];
        assert!(select_caption_track(&tracks, "en").is_none());
    }

    #[test]
    fn test_select_skips_tracks_without_url() {
        let tracks = vec![
            json!({"languageCode": "en"}), // no baseUrl
            make_track("en", Some("asr"), "http://asr-en"),
        ];
        let (url, _, _) = select_caption_track(&tracks, "en").unwrap();
        assert_eq!(url, "http://asr-en");
    }

    // ── parse_caption_xml ────────────────────────────────────────────

    #[test]
    fn test_parse_caption_xml_basic() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?><transcript><text start="0" dur="2.5">Hello everyone</text><text start="2.5" dur="3.0">welcome to the video</text></transcript>"#;
        let result = parse_caption_xml(xml).unwrap();
        assert_eq!(result, "Hello everyone welcome to the video");
    }

    #[test]
    fn test_parse_caption_xml_with_entities() {
        let xml = r#"<transcript><text start="0" dur="1">Tom &amp; Jerry</text><text start="1" dur="1">said &quot;hi&quot;</text></transcript>"#;
        let result = parse_caption_xml(xml).unwrap();
        assert_eq!(result, r#"Tom & Jerry said "hi""#);
    }

    #[test]
    fn test_parse_caption_xml_skips_empty() {
        let xml = r#"<transcript><text start="0" dur="1">Hello</text><text start="1" dur="1">   </text><text start="2" dur="1">World</text></transcript>"#;
        let result = parse_caption_xml(xml).unwrap();
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_parse_caption_xml_empty_input() {
        let result = parse_caption_xml("").unwrap();
        assert_eq!(result, "");
    }

    // ── decode_html_entities ─────────────────────────────────────────

    #[test]
    fn test_decode_named_entities() {
        assert_eq!(decode_html_entities("&amp;&lt;&gt;&quot;"), "&<>\"");
    }

    #[test]
    fn test_decode_apostrophe_variants() {
        assert_eq!(
            decode_html_entities("it&#39;s &apos;fine&apos;"),
            "it's 'fine'"
        );
    }

    #[test]
    fn test_decode_numeric_reference() {
        assert_eq!(decode_html_entities("&#65;&#66;&#67;"), "ABC");
    }

    #[test]
    fn test_decode_newlines_replaced() {
        assert_eq!(decode_html_entities("line1\nline2"), "line1 line2");
    }

    #[test]
    fn test_decode_nbsp() {
        assert_eq!(decode_html_entities("hello&nbsp;world"), "hello world");
    }

    #[test]
    fn test_decode_no_entities() {
        assert_eq!(decode_html_entities("plain text"), "plain text");
    }

    #[test]
    fn test_decode_no_double_decode() {
        // &amp;lt; should become &lt;, NOT <
        assert_eq!(decode_html_entities("&amp;lt;"), "&lt;");
    }

    #[test]
    fn test_decode_hex_reference() {
        // &#x41; = 'A', &#x7B; = '{'
        assert_eq!(decode_html_entities("&#x41;&#x7B;"), "A{");
    }
}

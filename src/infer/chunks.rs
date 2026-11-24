use anyhow::{Context, Result};

pub const SAMPLE_RATE: u32 = 16_000;
pub const MAX_CHUNK_SECONDS: u64 = 35;
const TARGET_SECONDS: u64 = 25;
const SEARCH_SECONDS: u64 = 5;
const RATE: u64 = SAMPLE_RATE as u64;
const WINDOW: u64 = RATE / 10;
const MAX_SAMPLES: u64 = RATE * MAX_CHUNK_SECONDS;

/// Return the next exclusive cut without dropping or repeating any audio.
/// The callback supplies the amplitude sum of a 100 ms PCM window.
pub fn next_chunk_end(
    start: u64,
    sample_count: u64,
    mut energy: impl FnMut(u64, u64) -> Result<u64>,
) -> Result<u64> {
    if sample_count - start <= MAX_SAMPLES {
        return Ok(sample_count);
    }
    let low = start + RATE * (TARGET_SECONDS - SEARCH_SECONDS);
    let high = start + RATE * (TARGET_SECONDS + SEARCH_SECONDS);
    let mut best = start + RATE * TARGET_SECONDS;
    let mut best_energy = u64::MAX;
    let mut cursor = low / WINDOW * WINDOW;
    while cursor + WINDOW <= high {
        let level = energy(cursor, WINDOW)?;
        if level < best_energy {
            best_energy = level;
            best = cursor + WINDOW / 2;
        }
        cursor += WINDOW;
    }
    Ok(best)
}

pub fn pcm_energy(samples: impl Iterator<Item = i16>) -> u64 {
    samples.map(|sample| u64::from(sample.unsigned_abs())).sum()
}

pub fn next_pcm_chunk_len(samples: &[i16], start: u64) -> usize {
    next_chunk_end(start, start + samples.len() as u64, |offset, count| {
        let offset = (offset - start) as usize;
        Ok(pcm_energy(
            samples[offset..offset + count as usize].iter().copied(),
        ))
    })
    .expect("measuring PCM already in memory is infallible") as usize
        - start as usize
}

pub fn append_chunk_text(transcript: &mut String, text: &str) {
    let text = text.trim();
    if !text.is_empty() {
        if !transcript.is_empty() {
            transcript.push(' ');
        }
        transcript.push_str(text);
    }
}

/// Transcribe bounded chunks in one worker and publish only the complete result.
pub fn transcribe_pcm_chunks(
    samples: &[i16],
    mut transcribe: impl FnMut(&[i16]) -> Result<String>,
) -> Result<String> {
    if samples.len() as u64 <= MAX_SAMPLES {
        return transcribe(samples);
    }

    let mut transcript = String::new();
    transcribe_remaining(samples, 0, &mut transcript, transcribe)?;
    Ok(transcript)
}

pub fn transcribe_remaining(
    mut samples: &[i16],
    mut start: u64,
    transcript: &mut String,
    mut transcribe: impl FnMut(&[i16]) -> Result<String>,
) -> Result<()> {
    while !samples.is_empty() {
        let len = next_pcm_chunk_len(samples, start);
        let text = transcribe(&samples[..len])
            .with_context(|| format!("transcribing chunk at {:.1}s", start as f64 / RATE as f64))?;
        append_chunk_text(transcript, &text);
        samples = &samples[len..];
        start += len as u64;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_recording_is_forwarded_unchanged() {
        let samples = [1, -1, 2, 3];
        let transcript = transcribe_pcm_chunks(&samples, |pcm| {
            assert_eq!(pcm, samples);
            Ok(" short result ".to_owned())
        })
        .unwrap();
        assert_eq!(transcript, " short result ");
    }

    #[test]
    fn long_recording_uses_quiet_cuts_and_preserves_the_tail() {
        let start_of_pause = (RATE * 24) as usize;
        let end_of_pause = start_of_pause + WINDOW as usize;
        let mut samples = vec![100; (RATE * 59) as usize + 3];
        samples[start_of_pause..end_of_pause].fill(0);
        samples[(RATE * 59) as usize..].copy_from_slice(&[7, 8, 9]);
        let mut consumed = 0;
        let transcript = transcribe_pcm_chunks(&samples, |chunk| {
            assert!(!chunk.is_empty());
            assert!(chunk.len() as u64 <= MAX_SAMPLES);
            assert_eq!(&samples[consumed..consumed + chunk.len()], chunk);
            if consumed == 0 {
                assert_eq!(chunk.len(), start_of_pause + WINDOW as usize / 2);
            }
            consumed += chunk.len();
            Ok(" repeated words ".to_owned())
        })
        .unwrap();
        assert_eq!(consumed, samples.len());
        assert_eq!(transcript, "repeated words repeated words");
    }
}

use crate::infer::chunks::{append_chunk_text, next_pcm_chunk_len, transcribe_remaining};
use anyhow::{anyhow, Result};
use crossbeam_channel::{bounded, Receiver, Sender, TryRecvError};

/// Audio stays in the recording buffer until its transcript is available.
#[derive(Debug, Default)]
pub struct RecordingChunks {
    pending: Option<PendingChunk>,
    completed_samples: u64,
    completed_chunks: usize,
    transcript: String,
    retry_at_stop: bool,
}

#[derive(Debug)]
struct PendingChunk {
    len: usize,
    result: Receiver<Result<String>>,
}

pub struct ChunkTask {
    pub samples: Vec<i16>,
    pub result: Sender<Result<String>>,
}

impl RecordingChunks {
    pub fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    pub fn recorded_samples(&self, buffered_samples: usize) -> u64 {
        self.completed_samples + buffered_samples as u64
    }

    pub fn queue(&mut self, samples: &[i16]) -> Option<ChunkTask> {
        if self.is_pending() || self.retry_at_stop {
            return None;
        }
        let len = next_pcm_chunk_len(samples, self.completed_samples);
        if len == samples.len() {
            return None;
        }
        let (result, receive) = bounded(1);
        self.pending = Some(PendingChunk {
            len,
            result: receive,
        });
        Some(ChunkTask {
            samples: samples[..len].to_vec(),
            result,
        })
    }

    pub fn poll(&mut self, samples: &mut Vec<i16>) -> Option<Result<(usize, String)>> {
        let pending = self.pending.as_ref()?;
        let result = match pending.result.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return None,
            Err(TryRecvError::Disconnected) => Err(anyhow!("recording chunk task stopped")),
        };
        let len = pending.len;
        self.pending = None;
        Some(
            self.accept(samples, len, result)
                .map(|text| (self.completed_chunks, text)),
        )
    }

    pub fn finish(
        mut self,
        mut samples: Vec<i16>,
        mut transcribe: impl FnMut(&[i16]) -> Result<String>,
    ) -> Result<String> {
        if let Some(pending) = self.pending.take() {
            let result = pending
                .result
                .recv()
                .unwrap_or_else(|_| Err(anyhow!("recording chunk task stopped")));
            if let Err(error) = self.accept(&mut samples, pending.len, result) {
                tracing::warn!(%error, "retrying recording chunk after stop");
            }
        }
        if self.completed_samples == 0 {
            return transcribe(&samples);
        }
        transcribe_remaining(
            &samples,
            self.completed_samples,
            &mut self.transcript,
            transcribe,
        )?;
        Ok(self.transcript)
    }

    fn accept(
        &mut self,
        samples: &mut Vec<i16>,
        len: usize,
        result: Result<String>,
    ) -> Result<String> {
        match result {
            Ok(text) => {
                append_chunk_text(&mut self.transcript, &text);
                self.completed_samples += len as u64;
                self.completed_chunks += 1;
                samples.drain(..len);
                Ok(text)
            }
            Err(error) => {
                self.retry_at_stop = true;
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::infer::chunks::{transcribe_pcm_chunks, SAMPLE_RATE};

    #[test]
    fn live_and_release_only_use_identical_boundaries() {
        let second = SAMPLE_RATE as usize;
        let audio = (0..103 * second)
            .map(|index| (1 + index % 47) as i16)
            .collect::<Vec<_>>();
        let describe = |chunk: &[i16]| Ok(format!("{}-{}", chunk.len(), chunk[0]));
        let expected = transcribe_pcm_chunks(&audio, describe).unwrap();
        let mut live = RecordingChunks::default();
        let mut samples = Vec::new();
        for frame in audio.chunks(second) {
            samples.extend_from_slice(frame);
            if let Some(task) = live.queue(&samples) {
                assert!(live.queue(&samples).is_none());
                task.result.send(describe(&task.samples)).unwrap();
                live.poll(&mut samples).unwrap().unwrap();
            }
        }
        assert!(live.completed_samples > 2 * second as u64);
        assert_eq!(live.recorded_samples(samples.len()), audio.len() as u64);
        assert_eq!(live.finish(samples, describe).unwrap(), expected);
    }

    #[test]
    fn stop_retries_failed_in_flight_audio_without_repeating_completed_text() {
        let second = SAMPLE_RATE as usize;
        let mut live = RecordingChunks::default();
        let mut samples = vec![1; 41 * second];
        let first = live.queue(&samples).unwrap();
        first.result.send(Ok(" first ".into())).unwrap();
        live.poll(&mut samples).unwrap().unwrap();
        samples.resize(38 * second, 2);
        let second_task = live.queue(&samples).unwrap();
        let failed_prefix = second_task.samples;
        second_task
            .result
            .send(Err(anyhow!("test failure")))
            .unwrap();
        let mut seen = Vec::new();
        let transcript = live
            .finish(samples, |chunk| {
                seen.push(chunk.to_vec());
                Ok("retry".into())
            })
            .unwrap();
        assert_eq!(seen[0], failed_prefix);
        assert_eq!(seen.iter().map(Vec::len).sum::<usize>(), 38 * second);
        assert_eq!(transcript, "first retry retry");
    }
}

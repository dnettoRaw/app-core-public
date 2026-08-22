// =============================================================================
//        #######
//     ###       ###     F: candle_batch.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use super::*;

impl CandleBackend {
    pub(super) fn infer_batch_sync(
        &self,
        requests: &[AiRequest],
        model: &ModelDescriptor,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<AiResult<AiResponse>>> {
        validate_batch_size(requests.len())?;
        if requests.is_empty() {
            return Ok(Vec::new());
        }
        check_cancellation(cancellation)?;
        if let [request] = requests {
            return Ok(vec![self.infer_sync(request, model, cancellation)]);
        }
        let loaded = self.loaded_model(model)?;
        let (mut outcomes, indices, features) = self.prepare_batch(requests, &loaded);
        if indices.is_empty() {
            return Ok(collect_outcomes(outcomes));
        }
        let started = Instant::now();
        let _active = ActiveInference::new(&self.active);
        let count = indices.len();
        let probabilities = self.execute_batch(features, count, &loaded)?;
        check_cancellation(cancellation)?;
        for (index, probabilities) in indices.into_iter().zip(probabilities) {
            outcomes[index] = Some(self.response(model, &loaded, probabilities));
        }
        self.inference_count
            .fetch_add(u64::try_from(count).unwrap_or(u64::MAX), Ordering::Relaxed);
        update_ema(&self.latency_ema_ms, elapsed_ms(started));
        Ok(collect_outcomes(outcomes))
    }

    fn prepare_batch(
        &self,
        requests: &[AiRequest],
        loaded: &LoadedLinear,
    ) -> (Vec<Option<AiResult<AiResponse>>>, Vec<usize>, Vec<f32>) {
        let mut outcomes = (0..requests.len()).map(|_| None).collect::<Vec<_>>();
        let mut indices = Vec::with_capacity(requests.len());
        let mut features =
            Vec::with_capacity(requests.len().saturating_mul(loaded.input_dimensions));
        for (index, request) in requests.iter().enumerate() {
            match self.request_text(request) {
                Ok(text) => {
                    indices.push(index);
                    features.extend(text_features(text, loaded.input_dimensions));
                }
                Err(error) => outcomes[index] = Some(Err(error)),
            }
        }
        (outcomes, indices, features)
    }

    fn execute_batch(
        &self,
        features: Vec<f32>,
        count: usize,
        loaded: &LoadedLinear,
    ) -> AiResult<Vec<Vec<f32>>> {
        let input = Tensor::from_vec(features, (count, loaded.input_dimensions), &Device::Cpu)
            .map_err(|_| self.failure("tensor-batch-input"))?;
        let transposed = loaded
            .weights
            .t()
            .map_err(|_| self.failure("tensor-batch-transpose"))?;
        let logits = input
            .matmul(&transposed)
            .and_then(|value| value.broadcast_add(&loaded.biases))
            .map_err(|_| self.failure("linear-batch-inference"))?;
        candle_nn::ops::softmax_last_dim(&logits)
            .and_then(|value| value.to_vec2::<f32>())
            .map_err(|_| self.failure("linear-batch-softmax"))
    }
}

fn validate_batch_size(size: usize) -> AiResult<()> {
    if size > CANDLE_LINEAR_MAX_BATCH_SIZE {
        return Err(AiError::LimitExceeded {
            kind: crate::LimitKind::InputParts,
            actual: u64::try_from(size).unwrap_or(u64::MAX),
            limit: u64::try_from(CANDLE_LINEAR_MAX_BATCH_SIZE).unwrap_or(u64::MAX),
        });
    }
    Ok(())
}

fn collect_outcomes(outcomes: Vec<Option<AiResult<AiResponse>>>) -> Vec<AiResult<AiResponse>> {
    outcomes
        .into_iter()
        .map(|outcome| outcome.unwrap_or(Err(AiError::InternalState)))
        .collect()
}

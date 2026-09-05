//! Local admission ledger and interpretable online safety calibration.
#![forbid(unsafe_code)]
use inferqos_core::WorkUnits;
use parking_lot::Mutex;
use serde::Serialize;
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct CapacityStatus {
    pub configured_units: f64,
    pub reserved_units: f64,
    pub safety_factor: f64,
    pub estimate_error_ewma: f64,
    pub throttle_ewma: f64,
    pub confidence: f64,
    pub observations: u64,
}
struct State {
    reservations: HashMap<Uuid, f64>,
    reserved: f64,
    safety: f64,
    error: f64,
    throttle: f64,
    observations: u64,
}
pub struct CapacityModel {
    configured: f64,
    alpha: f64,
    state: Mutex<State>,
}
impl CapacityModel {
    pub fn new(configured: f64, initial_safety: f64) -> Self {
        Self {
            configured,
            alpha: 0.08,
            state: Mutex::new(State {
                reservations: HashMap::new(),
                reserved: 0.0,
                safety: initial_safety.clamp(1.0, 4.0),
                error: 0.0,
                throttle: 0.0,
                observations: 0,
            }),
        }
    }
    pub fn reserve(&self, predicted: WorkUnits) -> Option<Uuid> {
        let mut s = self.state.lock();
        let charged = predicted.0 * s.safety;
        if !charged.is_finite() || charged < 0.0 || s.reserved + charged > self.configured {
            return None;
        }
        let id = Uuid::new_v4();
        s.reserved += charged;
        s.reservations.insert(id, charged);
        Some(id)
    }
    pub fn release(
        &self,
        id: Uuid,
        predicted: WorkUnits,
        actual: Option<WorkUnits>,
        throttled: bool,
    ) -> bool {
        let mut s = self.state.lock();
        let Some(charged) = s.reservations.remove(&id) else {
            return false;
        };
        s.reserved = (s.reserved - charged).max(0.0);
        let ratio = actual
            .map_or(1.0, |a| a.0 / predicted.0.max(1.0))
            .clamp(0.0, 10.0);
        s.error = (1.0 - self.alpha) * s.error + self.alpha * (ratio - 1.0).abs();
        s.throttle = (1.0 - self.alpha) * s.throttle + self.alpha * f64::from(throttled);
        let target = (1.0 + s.error * 1.5 + s.throttle * 2.0).clamp(1.0, 4.0);
        s.safety = (s.safety * 0.9 + target * 0.1).clamp(1.0, 4.0);
        s.observations += 1;
        true
    }
    pub fn status(&self) -> CapacityStatus {
        let s = self.state.lock();
        CapacityStatus {
            configured_units: self.configured,
            reserved_units: s.reserved,
            safety_factor: s.safety,
            estimate_error_ewma: s.error,
            throttle_ewma: s.throttle,
            confidence: (s.observations as f64 / 100.0).min(1.0),
            observations: s.observations,
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hard_limit_never_exceeded() {
        let m = CapacityModel::new(10.0, 1.0);
        assert!(m.reserve(WorkUnits(6.0)).is_some());
        assert!(m.reserve(WorkUnits(5.0)).is_none());
        assert!(m.status().reserved_units <= 10.0);
    }
    #[test]
    fn release_is_idempotent() {
        let m = CapacityModel::new(10.0, 1.0);
        let id = m.reserve(WorkUnits(2.0)).unwrap();
        assert!(m.release(id, WorkUnits(2.0), None, false));
        assert!(!m.release(id, WorkUnits(2.0), None, false));
    }
}

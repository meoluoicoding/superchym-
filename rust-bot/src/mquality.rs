use std::sync::atomic::{AtomicPtr, Ordering};
use crate::types::*;

static G_MQUALITY: AtomicPtr<MQualityGrid> = AtomicPtr::new(std::ptr::null_mut());
static G_GEOMETRY: AtomicPtr<GeometryConfig> = AtomicPtr::new(std::ptr::null_mut());

pub fn set_mquality(grid: MQualityGrid) {
    let boxed = Box::into_raw(Box::new(grid));
    let old = G_MQUALITY.swap(boxed, Ordering::Relaxed);
    if !old.is_null() {
        unsafe { drop(Box::from_raw(old)); }
    }
}

pub fn set_geometry(cfg: GeometryConfig) {
    let boxed = Box::into_raw(Box::new(cfg));
    let old = G_GEOMETRY.swap(boxed, Ordering::Relaxed);
    if !old.is_null() {
        unsafe { drop(Box::from_raw(old)); }
    }
}

pub fn get_mquality() -> Option<&'static MQualityGrid> {
    let ptr = G_MQUALITY.load(Ordering::Relaxed);
    if ptr.is_null() {
        None
    } else {
        Some(unsafe { &*ptr })
    }
}

pub fn get_geometry() -> GeometryConfig {
    let ptr = G_GEOMETRY.load(Ordering::Relaxed);
    if ptr.is_null() {
        GeometryConfig {
            corner_weight: 10,
            edge_weight: 5,
            center_weight: 3,
            connectivity_weight: 8,
            steal_weight: 12,
            barrier_weight: 15,
            compact_bonus: 6,
            risk_penalty: 10,
        }
    } else {
        unsafe { *ptr }
    }
}

#[inline]
pub fn cell_quality(values: &ValueGrid, r: usize, c: usize) -> i32 {
    if let Some(q) = get_mquality() {
        q[r][c] as i32
    } else {
        values[r][c] as i32
    }
}

#[inline]
pub fn move_quality_sum(values: &ValueGrid, r1: i32, c1: i32, r2: i32, c2: i32) -> i32 {
    let mut sum = 0i32;
    for r in r1..=r2 {
        for c in c1..=c2 {
            sum += cell_quality(values, r as usize, c as usize);
        }
    }
    sum
}

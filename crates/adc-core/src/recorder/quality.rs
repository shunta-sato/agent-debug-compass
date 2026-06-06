use crate::{ClockConfidence, DataQuality};

pub(super) fn medium_quality() -> DataQuality {
    DataQuality {
        clock_confidence: ClockConfidence::Medium,
        ..Default::default()
    }
}

pub(super) fn data_quality_for_drop_count(drop_count: u64) -> DataQuality {
    let mut data_quality = medium_quality();
    if drop_count > 0 {
        data_quality.dropped = true;
        data_quality.drop_count = drop_count;
        data_quality
            .notes
            .push("memory ring dropped oldest samples".to_string());
    }
    data_quality
}

#[test]
fn test_exit_status_rkyv_roundtrip() {
    use mswea_core::ExitStatus;

    let variants = [
        ExitStatus::Submitted,
        ExitStatus::LimitsExceeded,
        ExitStatus::UserInterruption,
        ExitStatus::FormatError,
        ExitStatus::ModelError,
        ExitStatus::EnvironmentError,
        ExitStatus::Uncaught,
    ];

    for original in variants {
        let bytes = rkyv::to_bytes::<rkyv::rancor::Error>(&original).unwrap();
        let decoded = rkyv::from_bytes::<ExitStatus, rkyv::rancor::Error>(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

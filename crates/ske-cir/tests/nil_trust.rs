use ske_cir::{proto::*, validate_literal};

#[test]
fn opaque_descriptors_cannot_authorize_null_constants() {
    let null = LiteralValue {
        representation: Some(literal_value::Representation::NullValue(true)),
    };
    for (kind, descriptor) in [
        ("OPAQUE", "go:map["),
        ("OPAQUE", "go:chan "),
        ("OPAQUE", "go:map[string]int"),
        ("OPAQUE", "go:chan int"),
        ("FUNCTION", "not-go:function"),
        ("FUNCTION", "go:func()"),
        ("SLICE", "not-go:slice"),
        ("SLICE", "go:[]int"),
    ] {
        let ty = TypeDef {
            id: "untrusted".into(),
            kind: kind.into(),
            canonical_descriptor: descriptor.into(),
            ..Default::default()
        };
        assert_eq!(
            validate_literal(&null, &ty).expect_err(descriptor).code,
            "E_LITERAL_WIDTH"
        );
    }
}

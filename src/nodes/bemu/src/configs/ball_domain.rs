pub fn instance_for_funct(funct: u32) -> Option<u32> {
    with_accelerator(|p| {
        p.ball_domain
            .balls
            .iter()
            .find(|ball| ball.instructions.iter().any(|instruction| instruction.funct7 == funct))
            .map(|ball| ball.id)
    })
}

use super::accelerator::with_accelerator;

pub fn ball_class_for_funct(funct7: u32) -> Option<String> {
    with_accelerator(|topology| {
        topology
            .ball_domain
            .balls
            .iter()
            .find(|ball| ball.instructions.iter().any(|entry| entry.funct7 == funct7))
            .map(|ball| ball.rtl_class.clone())
    })
}

pub fn mnemonic_for_funct(funct7: u32) -> Option<String> {
    with_accelerator(|topology| {
        topology
            .ball_domain
            .balls
            .iter()
            .flat_map(|ball| ball.instructions.iter())
            .find(|entry| entry.funct7 == funct7)
            .map(|entry| entry.mnemonic.clone())
    })
}

pub fn parameters<P: prost::Message + Default>(component: &str, instance: u32) -> P {
    with_accelerator(|topology| {
        let ball = topology
            .ball_domain
            .balls
            .iter()
            .find(|ball| ball.id == instance)
            .unwrap_or_else(|| panic!("missing Ball instance {instance}"));
        let parameter = ball.parameters.as_ref().expect("Ball parameters");
        assert_eq!(
            parameter.type_url,
            format!("type.googleapis.com/buckyball.balls.{component}.Params")
        );
        P::decode(parameter.value.as_slice()).expect("decode typed Ball parameters")
    })
}

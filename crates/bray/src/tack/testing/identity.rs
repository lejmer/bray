use bray_test_protocol::TestIdentity;

pub(super) fn identity_text(identity: &TestIdentity) -> String {
    let path = identity.declaration();
    let mut segments = path.module().segments().collect::<Vec<_>>();

    segments.push(path.name().as_str());

    format!(
        "{}/{}::{}",
        identity.product().package().as_str(),
        identity.product().name(),
        segments.join(".")
    )
}

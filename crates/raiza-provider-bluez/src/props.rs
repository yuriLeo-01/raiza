//! Leitura tolerante de propriedades D-Bus (`a{sv}`).

use std::collections::HashMap;

use zbus::zvariant::OwnedValue;

pub(crate) type Props = HashMap<String, OwnedValue>;

pub(crate) fn string(props: &Props, key: &str) -> Option<String> {
    props
        .get(key)
        .and_then(|v| <&str>::try_from(v).ok())
        .map(str::to_owned)
}

pub(crate) fn boolean(props: &Props, key: &str) -> Option<bool> {
    props.get(key).and_then(|v| bool::try_from(v).ok())
}

pub(crate) fn byte(props: &Props, key: &str) -> Option<u8> {
    props.get(key).and_then(|v| u8::try_from(v).ok())
}

pub(crate) fn uint(props: &Props, key: &str) -> Option<u32> {
    props.get(key).and_then(|v| u32::try_from(v).ok())
}

pub(crate) fn double(props: &Props, key: &str) -> Option<f64> {
    props.get(key).and_then(|v| f64::try_from(v).ok())
}

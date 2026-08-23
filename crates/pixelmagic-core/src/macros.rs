//! Internal macro for wiring a plain struct up to [`Parameterized`].
//!
//! Every adjustment and effect is "a struct of f32s plus a couple of colours",
//! and each needs a spec list, a getter and a setter that all agree with one
//! another. Writing those three by hand for ~90 node types is both tedious and
//! a reliable source of typos — a key that appears in `specs()` but not in
//! `set()` produces a control that silently does nothing. Declaring the fields
//! once removes that whole class of bug.

use crate::color::Rgba;
use crate::curve::Curve;
use crate::param::ParamValue;
use glam::Vec2;

/// Conversion between a struct field and the dynamic [`ParamValue`].
pub trait ParamField: Sized {
    fn to_param(&self) -> ParamValue;
    fn from_param(value: &ParamValue) -> Option<Self>;
}

impl ParamField for f32 {
    fn to_param(&self) -> ParamValue {
        ParamValue::Float(*self)
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_f32()
    }
}

impl ParamField for bool {
    fn to_param(&self) -> ParamValue {
        ParamValue::Bool(*self)
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_bool()
    }
}

impl ParamField for u32 {
    fn to_param(&self) -> ParamValue {
        ParamValue::Index(*self)
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_index()
    }
}

impl ParamField for Rgba {
    fn to_param(&self) -> ParamValue {
        ParamValue::Color(*self)
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_color()
    }
}

impl ParamField for Vec2 {
    fn to_param(&self) -> ParamValue {
        ParamValue::Point(*self)
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_point()
    }
}

impl ParamField for Curve {
    fn to_param(&self) -> ParamValue {
        ParamValue::Curve(self.clone())
    }
    fn from_param(value: &ParamValue) -> Option<Self> {
        value.as_curve().cloned()
    }
}

/// Declare a parameterised node.
///
/// ```ignore
/// parameterized! {
///     /// Doc comment lands on the generated struct.
///     pub struct Vignette {
///         exposure: f32 = "exposure", "Exposure", ParamKind::bipolar_percent();
///         softness: f32 = "softness", "Softness", ParamKind::unit_percent(0.5);
///     }
/// }
/// ```
///
/// Generates the struct, a `Default` built from each control's default value,
/// and a [`Parameterized`](crate::param::Parameterized) impl.
#[macro_export]
macro_rules! parameterized {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident {
            $(
                $(#[$fmeta:meta])*
                $field:ident : $fty:ty = $key:literal, $label:literal, $kind:expr
                $(, group = $group:literal)? ;
            )*
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, ::serde::Serialize, ::serde::Deserialize)]
        $vis struct $name {
            $(
                $(#[$fmeta])*
                pub $field: $fty,
            )*
        }

        impl Default for $name {
            fn default() -> Self {
                Self {
                    $(
                        $field: <$fty as $crate::macros::ParamField>::from_param(
                            &$kind.default_value()
                        ).expect(concat!(
                            "default value for `", $key,
                            "` does not match the declared field type"
                        )),
                    )*
                }
            }
        }

        impl $crate::param::Parameterized for $name {
            fn specs(&self) -> Vec<$crate::param::ParamSpec> {
                vec![$(
                    $crate::param::ParamSpec {
                        key: $key,
                        label: $label,
                        kind: $kind,
                        group: $crate::parameterized!(@group $($group)?),
                    },
                )*]
            }

            fn get(&self, key: &str) -> Option<$crate::param::ParamValue> {
                match key {
                    $($key => Some(
                        <$fty as $crate::macros::ParamField>::to_param(&self.$field)
                    ),)*
                    _ => None,
                }
            }

            fn set(&mut self, key: &str, value: $crate::param::ParamValue) -> bool {
                match key {
                    $($key => {
                        let clamped = $kind.clamp(value);
                        match <$fty as $crate::macros::ParamField>::from_param(&clamped) {
                            Some(v) => { self.$field = v; true }
                            None => false,
                        }
                    })*
                    _ => false,
                }
            }
        }
    };

    (@group) => { None };
    (@group $group:literal) => { Some($group) };
}

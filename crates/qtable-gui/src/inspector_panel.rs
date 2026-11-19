//! The closed set of Detail View tabs; every producer uses detail_layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Tab {
    Clip,
    Scene,
    Track,
    Output,
    Main,
}

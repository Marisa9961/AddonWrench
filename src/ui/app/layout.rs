use gpui::{Pixels, Size, px, size};

/// Keep dialogs inside the viewport with space for their backdrop on every edge.
pub(super) fn dialog_size(viewport: Size<Pixels>, preferred: Size<Pixels>) -> Size<Pixels> {
    size(
        preferred.width.min((viewport.width - px(48.)).max(px(0.))),
        preferred
            .height
            .min((viewport.height - px(48.)).max(px(0.))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dialogs_fit_small_viewports_and_retain_their_preferred_size_when_space_allows() {
        let preferred = size(px(600.), px(490.));
        assert_eq!(dialog_size(size(px(1080.), px(720.)), preferred), preferred);
        assert_eq!(
            dialog_size(size(px(500.), px(400.)), preferred),
            size(px(452.), px(352.))
        );
        assert_eq!(
            dialog_size(size(px(30.), px(30.)), preferred),
            size(px(0.), px(0.))
        );
    }
}

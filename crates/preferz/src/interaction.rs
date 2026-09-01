use crate::viewport::ViewportState;
use eframe::egui;
use preferz_core::item::ItemKind;
use preferz_core::{Item, Scene};

/// 在场景中从顶到底查找命中的最顶层 item（按 Z 序倒序）。
///
/// 命中检测走 [`Item::contains_canvas_point`]（OBB），正确处理旋转/翻转/缩放，
/// 与 spec §7.2 一致（修 B2/W2：旧实现用 AABB，旋转后命中错误）。
///
/// **绑定文本重定向**（Excalidraw 语义）：最顶层命中绑定文本时返回其容器，
/// 点击文字区域等价于点击容器——绑定文本随容器联动，不可独立选中/拖动；
/// 容器已删除的孤儿文本按自由文本处理，仍可命中。
pub fn get_item_at<'a>(
    screen_pos: egui::Pos2,
    scene: &'a Scene,
    viewport: &ViewportState,
) -> Option<&'a Item> {
    let canvas_pos = viewport.screen_to_canvas(screen_pos);
    // 边框命中的屏幕像素松弛阈值（Phase D）→ 画布单位。
    let frame_threshold = 6.0 / viewport.zoom;
    // items_by_z_order 升序，倒序遍历 = 从顶层到底层
    scene
        .items_by_z_order()
        .iter()
        .rev()
        .find(|&item| {
            if item.is_frame() {
                // 画框仅边框点选，内容区域穿透到下层
                item.frame_border_hit(canvas_pos, frame_threshold)
            } else {
                item.contains_canvas_point(canvas_pos)
            }
        })
        .map(|v| v as _)
        .and_then(|item| {
            if scene.is_bound_text(item) {
                // is_bound_text 保证容器存在；unwrap_or(item) 仅兜底借用安全
                match &item.kind {
                    ItemKind::Text {
                        container_id: Some(cid),
                        ..
                    } => scene.get_item(cid).or(Some(item)),
                    _ => Some(item),
                }
            } else {
                Some(item)
            }
        })
}

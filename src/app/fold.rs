//! 通用大纲折叠：基于 headings 索引的行可见性纯函数（md/org/html 源共用）。
//!
//! 约定：headings 按文档顺序、line 递增；`collapsed[i]` = true 表示标题 i 的
//! 子树（含子标题与其正文）不可见，但标题行本身可见。

use crate::plugin::HeadingEntry;

/// 行号 → 所属标题索引：最后一个 `line <= target` 的标题。
/// 首个标题之前的文档头（如 FILETAGS 行）返回 None。
pub fn heading_at(headings: &[HeadingEntry], line: usize) -> Option<usize> {
    headings.iter().rposition(|h| h.line <= line)
}

/// hi 的祖先标题索引链（外层在前；允许跳级：`* A` / `*** C` 中 C 的祖先是 A）。
fn ancestors(headings: &[HeadingEntry], mut hi: usize) -> Vec<usize> {
    let mut out = Vec::new();
    let mut lev = headings[hi].level;
    while let Some(j) = (0..hi).rev().find(|&j| headings[j].level < lev) {
        out.push(j);
        lev = headings[j].level;
        hi = j;
    }
    out
}

/// 行是否可见：所属标题及其祖先均未折叠；折叠的标题只保留标题行自身，
/// 其正文（非标题行）一并隐藏；文档头恒可见。
pub fn line_visible(headings: &[HeadingEntry], collapsed: &[bool], line: usize) -> bool {
    let Some(hi) = heading_at(headings, line) else {
        return true;
    };
    if collapsed[hi] && headings[hi].line != line {
        return false;
    }
    ancestors(headings, hi).into_iter().all(|j| !collapsed[j])
}

/// 展开 line 所属标题的全部祖先（跳转前调用，保证目标可见）。
pub fn unfold_ancestors(headings: &[HeadingEntry], collapsed: &mut [bool], line: usize) {
    let Some(hi) = heading_at(headings, line) else {
        return;
    };
    for j in ancestors(headings, hi) {
        collapsed[j] = false;
    }
}

/// 切换标题 i 的折叠状态。
pub fn toggle(headings: &[HeadingEntry], collapsed: &mut [bool], hi: usize) {
    let _ = headings;
    if let Some(c) = collapsed.get_mut(hi) {
        *c = !*c;
    }
}

/// 折叠 cycle：无折叠 → 收 2 级及以上；有折叠 → 全展开。
pub fn cycle(headings: &[HeadingEntry], collapsed: &mut [bool]) {
    let any = collapsed.iter().any(|&c| c);
    for (i, c) in collapsed.iter_mut().enumerate() {
        *c = !any && headings[i].level > 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hs() -> Vec<HeadingEntry> {
        vec![
            HeadingEntry {
                level: 1,
                text: "a".into(),
                line: 0,
            },
            HeadingEntry {
                level: 2,
                text: "a1".into(),
                line: 2,
            },
            HeadingEntry {
                level: 2,
                text: "a2".into(),
                line: 3,
            },
            HeadingEntry {
                level: 1,
                text: "b".into(),
                line: 5,
            },
        ]
    }

    #[test]
    fn collapse_hides_subtree_lines() {
        let h = hs();
        let mut c = vec![false; h.len()];
        c[0] = true; // 折叠 a（1 级）
        assert!(!line_visible(&h, &c, 1)); // a 的正文
        assert!(!line_visible(&h, &c, 2)); // a1
        assert!(!line_visible(&h, &c, 3)); // a2
        assert!(line_visible(&h, &c, 5)); // b 仍可见
    }

    #[test]
    fn heading_line_itself_stays_visible() {
        let h = hs();
        let mut c = vec![false; h.len()];
        c[0] = true;
        assert!(line_visible(&h, &c, 0)); // 折叠的标题行自身可见
    }

    #[test]
    fn unfold_ancestors_restores() {
        let h = hs();
        let mut c = vec![false; h.len()];
        c[0] = true;
        unfold_ancestors(&h, &mut c, 3);
        assert!(line_visible(&h, &c, 3));
        assert!(line_visible(&h, &c, 1));
    }

    #[test]
    fn header_before_first_heading_always_visible() {
        let h = vec![HeadingEntry {
            level: 1,
            text: "t".into(),
            line: 2,
        }];
        let c = vec![true];
        assert!(line_visible(&h, &c, 0));
        assert!(line_visible(&h, &c, 1));
        assert!(line_visible(&h, &c, 2)); // 折叠后标题行自身仍可见
    }

    #[test]
    fn cycle_collapses_level2_then_expands() {
        let h = hs();
        let mut c = vec![false; h.len()];
        cycle(&h, &mut c); // 无折叠 → 收 2 级及以上
        assert!(!c[0]);
        assert!(c[1]);
        assert!(c[2]);
        assert!(!c[3]);
        cycle(&h, &mut c); // → 全展开
        assert!(c.iter().all(|&x| !x));
    }

    #[test]
    fn heading_at_mid_body() {
        let h = hs();
        assert_eq!(heading_at(&h, 1), Some(0)); // a 的正文
        assert_eq!(heading_at(&h, 4), Some(2)); // a2 与 b 之间
        assert_eq!(heading_at(&h, 5), Some(3)); // b 标题行
        assert_eq!(heading_at(&h, 99), Some(3)); // 尾部
    }

    #[test]
    fn skipped_level_parent() {
        // * A / *** C（跳级）：C 的祖先是 A
        let h = vec![
            HeadingEntry {
                level: 1,
                text: "A".into(),
                line: 0,
            },
            HeadingEntry {
                level: 3,
                text: "C".into(),
                line: 1,
            },
        ];
        let mut c = vec![false, false];
        c[0] = true;
        assert!(!line_visible(&h, &c, 1));
        unfold_ancestors(&h, &mut c, 1);
        assert!(line_visible(&h, &c, 1));
    }
}

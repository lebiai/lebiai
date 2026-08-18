//! When to keep, boost, or drop per-session material focus.

pub fn is_followup_query(text: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }
    if t.chars().count() <= 12 {
        return true;
    }
    t.starts_with('那')
        || t.starts_with("这个")
        || t.starts_with("还有")
        || t.starts_with("以及")
        || t.starts_with("刚才")
        || t.starts_with("上面")
        || t.starts_with("同份")
        || t.to_lowercase().starts_with("and the")
        || t.to_lowercase().starts_with("what about")
}

pub fn is_topic_reset(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    matches!(
        t.as_str(),
        "你好"
            | "您好"
            | "嗨"
            | "哈喽"
            | "hi"
            | "hello"
            | "hey"
            | "谢谢"
            | "谢谢你"
            | "thanks"
            | "thank you"
            | "好的"
            | "嗯"
            | "哦"
    ) || t.contains("今天天气")
        || t.contains("几点了")
}

pub fn wants_other_file(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("不是这份")
        || t.contains("换一份")
        || t.contains("另一份")
        || t.contains("不是这个文件")
        || t.contains("not this file")
        || t.contains("wrong file")
}

/// User said keep this for next time (non-Word/PDF, or after the fact).
pub fn wants_keep(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("留下")
        || t.contains("收着")
        || t.contains("以后按这个")
        || t.contains("下次还用")
        || t.contains("下次还按")
        || t.contains("keep this")
        || t.contains("keep it for next")
        || t.contains("save this file")
}

/// User asked what files are on hand.
pub fn wants_on_hand(text: &str) -> bool {
    let t = text.to_lowercase();
    t.contains("手头有哪些")
        || t.contains("手头有什么")
        || t.contains("有我哪些材料")
        || t.contains("留下了哪些")
        || t.contains("留下了什么")
        || t.contains("你有哪些材料")
        || t.contains("what materials")
        || t.contains("what files do you have")
}

/// User asked to remember a standard from the material — pending review, not auto-write.
pub fn wants_remember_standard(text: &str) -> bool {
    let t = text.to_lowercase();
    (t.contains("记住") || t.contains("remember"))
        && (t.contains("标准")
            || t.contains("口径")
            || t.contains("习惯")
            || t.contains("以后")
            || t.contains("this standard")
            || t.contains("this rule"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn followup_and_reset() {
        assert!(is_followup_query("那逾期呢？"));
        assert!(is_followup_query("还有呢"));
        assert!(is_topic_reset("你好"));
        assert!(is_topic_reset("谢谢"));
        assert!(!is_topic_reset("按对外口径写一条朋友圈"));
        assert!(wants_other_file("不是这份，换合同"));
        assert!(wants_keep("这份留下，下次还用"));
        assert!(wants_on_hand("你手头有我哪些材料？"));
        assert!(wants_remember_standard("记住这个标准，以后标题先写冲突"));
        assert!(!wants_remember_standard("记住周五交方案"));
    }
}

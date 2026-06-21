use anyhow::anyhow;
use std::fmt::Debug;

/// 将多个错误合并为一个 `anyhow::Error`，并在首行展示自定义的摘要信息。
///
/// `summary` 会被放在最终错误消息的第一行。
/// 后续每一行会以 `-` 开头，展示对应错误的 `{:#?}` 美化格式。
pub fn join_errors_with_summary<E: Debug>(
    summary: impl Into<String>,
    errors: &[E],
) -> anyhow::Error {
    let summary = summary.into();
    let mut lines = Vec::with_capacity(1 + errors.len());
    lines.push(summary);
    for e in errors {
        lines.push(format!("- {e:#?}"));
    }
    anyhow!("{}", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Error;
    use insta::assert_debug_snapshot;

    /// 一个简单的测试错误类型，用于验证 `{:#?}` 多行美化输出。
    #[derive(Debug)]
    struct TestError {
        code: i32,
        message: &'static str,
    }

    // 辅助函数：获取 anyhow::Error 的展示字符串
    fn display(err: &Error) -> String {
        format!("{err}")
    }

    #[test]
    fn empty_errors_only_shows_summary() {
        let summary = "没有发生任何错误";
        let errors: Vec<&str> = vec![];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        assert_eq!(msg, summary, "空错误列表应仅包含摘要信息");
    }

    #[test]
    fn single_error_contains_summary_and_detail() {
        let summary = "操作失败，发现 1 个错误：";
        let errors = vec!["文件未找到"];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        assert_debug_snapshot!(msg,@r#""操作失败，发现 1 个错误：\n- \"文件未找到\"""#);
    }

    #[test]
    fn multiple_errors_preserves_order_and_count() {
        let summary = "共 3 个错误：";
        let errors = vec!["err A", "err B", "err C"];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        let lines: Vec<&str> = msg.lines().collect();
        assert_eq!(lines.len(), 4, "应包含摘要行和 3 个详细行");
        assert_debug_snapshot!(msg,@r#""共 3 个错误：\n- \"err A\"\n- \"err B\"\n- \"err C\"""#);
    }

    #[test]
    fn summary_accepts_string_type() {
        let summary = String::from("自定义摘要");
        let errors = vec![404];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        assert_debug_snapshot!(msg,@r#""自定义摘要\n- 404""#);
    }

    #[test]
    fn custom_error_beautified_multiline() {
        let summary = "批量处理出错";
        let errors = vec![
            TestError {
                code: 404,
                message: "Not Found",
            },
            TestError {
                code: 500,
                message: "Internal Error",
            },
        ];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        // 验证使用了 `{:#?}` 多行美化格式
        assert_debug_snapshot!(msg,@r#""批量处理出错\n- TestError {\n    code: 404,\n    message: \"Not Found\",\n}\n- TestError {\n    code: 500,\n    message: \"Internal Error\",\n}""#);
    }

    #[test]
    fn empty_summary_is_allowed() {
        let summary = "";
        let errors = vec!["err1"];
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        // 第一行是空字符串，因此整个消息会以换行符开头
        assert!(
            msg.starts_with('\n') || msg.is_empty(),
            "空摘要应导致首行为空"
        );
        assert_debug_snapshot!(msg,@r#""\n- \"err1\"""#);
    }

    #[test]
    fn errors_with_complex_debug_output_work() {
        // 使用元组结构体或枚举，验证 Debug 实现被完整保留
        #[derive(Debug)]
        enum ComplexError {
            Io { path: String, code: i32 },
            Parse(String),
        }
        let errors = vec![
            ComplexError::Io {
                path: "/tmp/a".into(),
                code: 5,
            },
            ComplexError::Parse("invalid utf8".into()),
        ];

        let summary = "复杂错误";
        let err = join_errors_with_summary(summary, &errors);
        let msg = display(&err);
        assert_debug_snapshot!(msg,@r#""复杂错误\n- Io {\n    path: \"/tmp/a\",\n    code: 5,\n}\n- Parse(\n    \"invalid utf8\",\n)""#);
    }
}

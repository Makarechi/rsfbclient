use rsfbclient_core::{sql_params::sql_parameters, FbError, SqlType};

pub(super) fn is_execute_block(sql: &str) -> bool {
    fn trivia(mut sql: &str) -> &str {
        loop {
            sql = sql.trim_start();
            if let Some(tail) = sql.strip_prefix("--") {
                sql = tail.find(['\r', '\n']).map_or("", |i| &tail[i..]);
            } else if let Some(tail) = sql.strip_prefix("/*") {
                sql = tail.find("*/").map_or("", |i| &tail[i + 2..]);
            } else {
                return sql;
            }
        }
    }
    let sql = trivia(sql);
    let end = sql
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(sql.len());
    if !sql[..end].eq_ignore_ascii_case("EXECUTE") {
        return false;
    }
    let sql = trivia(&sql[end..]);
    let end = sql
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(sql.len());
    sql[..end].eq_ignore_ascii_case("BLOCK")
}

/// A UTF8 input VARCHAR can carry at most 8191 characters. A text BLOB input
/// avoids that message limit; the surrounding SQL still applies target types.
pub(super) fn large_text_indices(params: &[SqlType]) -> Vec<usize> {
    params
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            matches!(p,SqlType::Text(s) if rsfbclient_core::requires_text_blob(s)).then_some(i)
        })
        .collect()
}

pub(super) fn prepare_sql(sql: &str, indices: &[usize], count: usize) -> Result<String, FbError> {
    if indices.is_empty() {
        return Ok(sql.into());
    }
    let markers = sql_parameters(sql)?
        .into_iter()
        .filter(|m| m.name.is_none())
        .collect::<Vec<_>>();
    if markers.len() != count {
        return Err("The SQL parameter count does not match the supplied values".into());
    }
    let mut result = String::with_capacity(sql.len() + indices.len() * 56);
    let mut start = 0;
    for &i in indices {
        let range = &markers[i].range;
        result.push_str(&sql[start..range.start]);
        result.push_str("CAST(? AS BLOB SUB_TYPE TEXT CHARACTER SET UTF8)");
        start = range.end;
    }
    result.push_str(&sql[start..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adapts_only_real_markers_without_embedding_values() {
        let sql = "insert into t values(?, '?', q'{?}', ?) /* ? */";
        let indices =
            large_text_indices(&[SqlType::Text("s".into()), SqlType::Text("x".repeat(8192))]);
        assert_eq!(indices, vec![1]);
        assert_eq!(prepare_sql(sql,&indices,2).unwrap(),"insert into t values(?, '?', q'{?}', CAST(? AS BLOB SUB_TYPE TEXT CHARACTER SET UTF8)) /* ? */");
        assert!(prepare_sql(sql, &indices, 3).is_err());
    }
    #[test]
    fn limit_counts_characters_and_leaves_other_values_alone() {
        assert!(large_text_indices(&[
            SqlType::Text("😀".repeat(8191)),
            SqlType::Null,
            SqlType::Binary(vec![0; 10000]),
            SqlType::Octets(vec![0; 10000])
        ])
        .is_empty());
        assert_eq!(
            large_text_indices(&[SqlType::Text("😀".repeat(8192))]),
            vec![0]
        );
    }
    #[test]
    fn execute_block_has_explicit_parameter_declarations_not_expression_slots() {
        assert!(is_execute_block(
            "-- before\r /* note */ execute /* note */ block(v varchar(10)=?) as begin end"
        ));
        assert!(!is_execute_block("execute procedure p(?)"));
        assert!(!is_execute_block("select 'execute block' from t"));
    }
}

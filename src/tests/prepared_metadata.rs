//! Metadata and wide input regressions shared by the native and wire drivers.
mk_tests_default! {
    use crate::{prelude::*, FbError, Row, SimpleConnection};
    use rsfbclient_core::{ibase, StmtType};

    #[test]
    fn prepared_description_reports_origins_without_executing() -> Result<(), FbError> {
        let mut conn: SimpleConnection = cbuilder().connect()?.into();
        conn.execute("CREATE TABLE DESCRIBE_ORIGIN (ID INTEGER)", ())?;
        conn.execute("CREATE SEQUENCE DESCRIBE_PROBE", ())?;
        let sql = "SELECT X.ID AS \"Key name\", GEN_ID(DESCRIBE_PROBE, 1) FROM DESCRIBE_ORIGIN X";
        assert_eq!(conn.prepare_statement_type(sql)?, StmtType::Select);
        let columns = conn.describe_query_columns(sql)?;
        assert_eq!(columns[0].alias, "Key name");
        assert_eq!(columns[0].relation, "DESCRIBE_ORIGIN");
        assert_eq!(columns[0].field, "ID");
        assert!(columns[1].relation.is_empty());
        let (current,): (i64,) = conn.query_first(
            "SELECT GEN_ID(DESCRIBE_PROBE, 0) FROM RDB$DATABASE", (),
        )?.unwrap();
        assert_eq!(current, 0);
        conn.close()?;
        let mut cleanup=cbuilder().connect()?;
        cleanup.execute("DROP TABLE DESCRIBE_ORIGIN", ())?;
        cleanup.execute("DROP SEQUENCE DESCRIBE_PROBE", ())?;
        Ok(())
    }

    #[test]
    fn long_text_can_use_character_and_text_blob_parameters() -> Result<(), FbError> {
        let mut conn = cbuilder().charset(rsfbclient_core::charset::WIN_1251).connect()?;
        conn.execute("RECREATE TABLE WIDE_INPUT_PROBE (ID INT PRIMARY KEY, NOTE VARCHAR(12000) CHARACTER SET ASCII, CONTENT BLOB SUB_TYPE TEXT CHARACTER SET UTF8)", ())?;
        let note = "abc".repeat(3000);
        let content = "Жёлтый текст ".repeat(1000);
        conn.execute("INSERT INTO WIDE_INPUT_PROBE VALUES(?, ?, ?)", (1i32, note.clone(), content.clone()))?;
        assert_eq!(conn.query_first::<_, (String,String)>("SELECT NOTE, CONTENT FROM WIDE_INPUT_PROBE WHERE ID=1", ())?, Some((note,content)));
        let sql = "UPDATE WIDE_INPUT_PROBE SET NOTE=? WHERE ID=1";
        conn.execute(sql, ("z".repeat(10000),))?;
        conn.execute(sql, ("short",))?;
        assert_eq!(conn.query_first::<_,(String,)>("SELECT NOTE FROM WIDE_INPUT_PROBE",())?,Some(("short".into(),)));
        conn.close()?;
        cbuilder().connect()?.execute("DROP TABLE WIDE_INPUT_PROBE", ())?;
        Ok(())
    }

    #[test]
    fn extended_scalars_keep_server_rendered_value_and_declared_type() -> Result<(), FbError> {
        use crate::{EngineVersion, SystemInfos};
        let mut conn = cbuilder().connect()?;
        if conn.server_engine()? < EngineVersion::V4 {
            return Ok(());
        }
        let row: Row = conn.query_first(
            "SELECT CAST('123456789012345678901234567890.1234' AS DECFLOAT(34)), TIMESTAMP '2025-01-01 12:30:00 Europe/Prague' FROM RDB$DATABASE", (),
        )?.unwrap();
        assert_eq!(row.cols[0].raw_type, ibase::SQL_DEC34);
        assert_eq!(row.get::<String>(0)?, "123456789012345678901234567890.1234");
        assert_eq!(row.cols[1].raw_type, ibase::SQL_TIMESTAMP_TZ);
        assert!(row.get::<String>(1)?.contains("Europe/Prague"));
        Ok(())
    }
}

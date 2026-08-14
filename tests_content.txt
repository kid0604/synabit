1321:mod tests {
1322-    use super::*;
1323-    use crate::db::schema::run_sync_schema_migrations;
1324-    use rusqlite::Connection;
1325-
1326-    fn setup_test_db() -> DbBridge {
1327-        let mut conn = Connection::open_in_memory().unwrap();
1328-        run_sync_schema_migrations(&mut conn).unwrap();
1329-
1330-        conn.execute(
1331-            "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v1', '/v1', 100, 100)",
1332-            [],
1333-        )
1334-        .unwrap();
1335-        conn.execute(
1336-            "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'gdrive', 100, 100)",
1337-            [],
1338-        )
1339-        .unwrap();
1340-
1341-        DbBridge { conn }
1342-    }
1343-
1344-    fn sample_entry(op_byte: u8, seq: u64) -> InboxEntryToStage {
1345-        InboxEntryToStage {
1346-            remote_position: format!("pos_{}", op_byte),
1347-            remote_seq: Some(seq),
1348-            operation_id: [op_byte; 16],
1349-            doc_hash: [op_byte; 32],
1350-            entry_kind: SyncEntryKind::Upsert,
1351-            encrypted_payload: Some(vec![10, 20, op_byte]),
1352-            payload_hash: Some([op_byte; 32]),
1353-            source_device: Some(format!("device_{}", op_byte)),
1354-        }
1355-    }
1356-
1357-    #[test]
1358-    fn insert_and_read_round_trip() {
1359-        let db = setup_test_db();
1360-        let entry = sample_entry(1, 42);
1361-
1362-        let res = db
1363-            .stage_inbox_page(
1364-                "v1",
1365-                "gdrive",
1366-                "cursor_1",
1367-                "cursor_2",
1368-                true,
1369-                &[entry.clone()],
1370-                1000,
1371-            )
1372-            .unwrap();
1373-
1374-        assert_eq!(res.inserted_count, 1);
1375-        assert_eq!(res.duplicate_count, 0);
1376-
1377-        let fetched = db
1378-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1379-            .unwrap()
1380-            .expect("Inbox record must exist");
1381-
1382-        assert_eq!(fetched.vault_id, "v1");
1383-        assert_eq!(fetched.provider_id, "gdrive");
1384-        assert_eq!(fetched.page_cursor, "cursor_1");
1385-        assert_eq!(fetched.remote_position, "pos_1");
1386-        assert_eq!(fetched.remote_seq, Some(42));
1387-        assert_eq!(fetched.operation_id, [1; 16]);
1388-        assert_eq!(fetched.doc_hash, [1; 32]);
1389-        assert_eq!(fetched.entry_kind, SyncEntryKind::Upsert);
1390-        assert_eq!(fetched.encrypted_payload, Some(vec![10, 20, 1]));
1391-        assert_eq!(fetched.payload_hash, Some([1; 32]));
1392-        assert_eq!(fetched.source_device, Some("device_1".to_string()));
1393-        assert_eq!(fetched.state, InboxState::Pending);
1394-        assert_eq!(fetched.last_error, None);
1395-        assert_eq!(fetched.received_at, 1000);
1396-        assert_eq!(fetched.updated_at, 1000);
1397-        assert_eq!(fetched.applied_at, None);
1398-    }
1399-
1400-    #[test]
1401-    fn transactional_page_staging_all_or_nothing() {
1402-        let db = setup_test_db();
1403-        let e1 = sample_entry(1, 10);
1404-        let e2 = sample_entry(2, 20);
1405-        let e3 = sample_entry(3, 30);
1406-
1407-        let res = db
1408-            .stage_inbox_page(
1409-                "v1",
1410-                "gdrive",
1411-                "cursor_batch",
1412-                "cursor_batch_next",
1413-                true,
1414-                &[e1, e2, e3],
1415-                1000,
1416-            )
1417-            .unwrap();
1418-
1419-        assert_eq!(res.inserted_count, 3);
1420-        assert_eq!(res.duplicate_count, 0);
1421-
1422-        assert!(db
1423-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1424-            .unwrap()
1425-            .is_some());
1426-        assert!(db
1427-            .get_inbox_by_id("v1", "gdrive", &[2; 16])
1428-            .unwrap()
1429-            .is_some());
1430-        assert!(db
1431-            .get_inbox_by_id("v1", "gdrive", &[3; 16])
1432-            .unwrap()
1433-            .is_some());
1434-    }
1435-
1436-    #[test]
1437-    fn transactional_page_staging_rollback_on_failure() {
1438-        let db = setup_test_db();
1439-        let e1 = sample_entry(1, 10);
1440-        let mut e2 = sample_entry(2, 20);
1441-        e2.remote_position = "".to_string(); // Invalid empty position causes failure
1442-        let e3 = sample_entry(3, 30);
1443-
1444-        let res = db.stage_inbox_page(
1445-            "v1",
1446-            "gdrive",
1447-            "cursor_batch",
1448-            "cursor_batch_next",
1449-            true,
1450-            &[e1, e2, e3],
1451-            1000,
1452-        );
1453-        assert!(res.is_err());
1454-
1455-        // Entire page must be rolled back: e1 and e3 must NOT exist in DB
1456-        assert!(db
1457-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1458-            .unwrap()
1459-            .is_none());
1460-        assert!(db
1461-            .get_inbox_by_id("v1", "gdrive", &[2; 16])
1462-            .unwrap()
1463-            .is_none());
1464-        assert!(db
1465-            .get_inbox_by_id("v1", "gdrive", &[3; 16])
1466-            .unwrap()
1467-            .is_none());
1468-    }
1469-
1470-    #[test]
1471-    fn idempotent_duplicate_page_staging() {
1472-        let db = setup_test_db();
1473-        let e1 = sample_entry(1, 10);
1474-
1475-        let res1 = db
1476-            .stage_inbox_page(
1477-                "v1",
1478-                "gdrive",
1479-                "cursor_1",
1480-                "cursor_2",
1481-                true,
1482-                &[e1.clone()],
1483-                1000,
1484-            )
1485-            .unwrap();
1486-        assert_eq!(res1.inserted_count, 1);
1487-        assert_eq!(res1.duplicate_count, 0);
1488-
1489-        let res2 = db
1490-            .stage_inbox_page(
1491-                "v1",
1492-                "gdrive",
1493-                "cursor_1",
1494-                "cursor_2",
1495-                true,
1496-                &[e1.clone()],
1497-                2000,
1498-            )
1499-            .unwrap();
1500-        assert_eq!(res2.inserted_count, 0);
1501-        assert_eq!(res2.duplicate_count, 1);
1502-
1503-        let fetched = db
1504-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1505-            .unwrap()
1506-            .unwrap();
1507-        assert_eq!(fetched.received_at, 1000);
1508-        assert_eq!(fetched.state, InboxState::Pending);
1509-    }
1510-
1511-    #[test]
1512-    fn duplicate_applied_entry_not_reset_to_pending() {
1513-        let db = setup_test_db();
1514-        let e1 = sample_entry(1, 10);
1515-
1516-        db.stage_inbox_page(
1517-            "v1",
1518-            "gdrive",
1519-            "cursor_1",
1520-            "cursor_2",
1521-            true,
1522-            &[e1.clone()],
1523-            1000,
1524-        )
1525-        .unwrap();
1526-
1527-        // Directly set state = applied in DB
1528-        db.conn
1529-            .execute(
1530-                "UPDATE sync_inbox SET state = 'applied', applied_at = 1500 WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND operation_id = ?1",
1531-                params![[1u8; 16]],
1532-            )
1533-            .unwrap();
1534-
1535-        // Stage duplicate
1536-        let res = db
1537-            .stage_inbox_page(
1538-                "v1",
1539-                "gdrive",
1540-                "cursor_1",
1541-                "cursor_2",
1542-                true,
1543-                &[e1.clone()],
1544-                2000,
1545-            )
1546-            .unwrap();
1547-        assert_eq!(res.inserted_count, 0);
1548-        assert_eq!(res.duplicate_count, 1);
1549-
1550-        let fetched = db
1551-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1552-            .unwrap()
1553-            .unwrap();
1554-        assert_eq!(fetched.state, InboxState::Applied);
1555-        assert_eq!(fetched.applied_at, Some(1500));
1556-    }
1557-
1558-    #[test]
1559-    fn duplicate_conflicting_content_causes_collision_error() {
1560-        let db = setup_test_db();
1561-        let e1 = sample_entry(1, 10);
1562-        let e2 = sample_entry(2, 20);
1563-
1564-        db.stage_inbox_page(
1565-            "v1",
1566-            "gdrive",
1567-            "cursor_1",
1568-            "cursor_2",
1569-            true,
1570-            &[e1.clone()],
1571-            1000,
1572-        )
1573-        .unwrap();
1574-
1575-        let mut e1_conflicting = e1.clone();
1576-        e1_conflicting.doc_hash = [99; 32];
1577-        e1_conflicting.remote_seq = Some(30);
1578-
1579-        // Batch containing e2 and conflicting e1
1580-        let res = db.stage_inbox_page(
1581-            "v1",
1582-            "gdrive",
1583-            "cursor_2",
1584-            "cursor_3",
1585-            true,
1586-            &[e2.clone(), e1_conflicting],
1587-            2000,
1588-        );
1589-        assert!(res.is_err());
1590-        let err_msg = res.unwrap_err().to_string();
1591-        assert!(err_msg.contains("collision"));
1592-
1593-        // Entire page 2 rolled back: e2 must NOT exist in DB
1594-        assert!(db
1595-            .get_inbox_by_id("v1", "gdrive", &[2; 16])
1596-            .unwrap()
1597-            .is_none());
1598-    }
1599-
1600-    #[test]
1601-    fn isolation_by_vault_and_provider() {
1602-        let db = setup_test_db();
1603-
1604-        db.conn
1605-            .execute(
1606-                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
1607-                [],
1608-            )
1609-            .unwrap();
1610-        db.conn
1611-            .execute(
1612-                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
1613-                [],
1614-            )
1615-            .unwrap();
1616-        db.conn
1617-            .execute(
1618-                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'gdrive', 200, 200)",
1619-                [],
1620-            )
1621-            .unwrap();
1622-
1623-        let e1 = sample_entry(1, 10);
1624-
1625-        // Same operation_id staged into (v1, gdrive), (v1, server), (v2, gdrive)
1626-        db.stage_inbox_page("v1", "gdrive", "c1", "c1_next", true, &[e1.clone()], 1000)
1627-            .unwrap();
1628-        db.stage_inbox_page("v1", "server", "c2", "c2_next", true, &[e1.clone()], 1000)
1629-            .unwrap();
1630-        db.stage_inbox_page("v2", "gdrive", "c3", "c3_next", true, &[e1.clone()], 1000)
1631-            .unwrap();
1632-
1633-        assert!(db
1634-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1635-            .unwrap()
1636-            .is_some());
1637-        assert!(db
1638-            .get_inbox_by_id("v1", "server", &[1; 16])
1639-            .unwrap()
1640-            .is_some());
1641-        assert!(db
1642-            .get_inbox_by_id("v2", "gdrive", &[1; 16])
1643-            .unwrap()
1644-            .is_some());
1645-    }
1646-
1647-    #[test]
1648-    fn foreign_key_to_provider_state_enforced() {
1649-        let db = setup_test_db();
1650-        let e1 = sample_entry(1, 10);
1651-
1652-        let res = db.stage_inbox_page("nonexistent_vault", "gdrive", "c1", "c2", true, &[e1], 1000);
1653-        assert!(res.is_err());
1654-    }
1655-
1656-    #[test]
1657-    fn empty_page_staging_returns_zero_counts() {
1658-        let db = setup_test_db();
1659-
1660-        let res = db
1661-            .stage_inbox_page(
1662-                "v1",
1663-                "gdrive",
1664-                "cursor_empty",
1665-                "cursor_empty_next",
1666-                true,
1667-                &[],
1668-                1000,
1669-            )
1670-            .unwrap();
1671-        assert_eq!(res.inserted_count, 0);
1672-        assert_eq!(res.duplicate_count, 0);
1673-    }
1674-
1675-    #[test]
1676-    fn stage_page_validations_reject_invalid_bounds() {
1677-        let db = setup_test_db();
1678-        let e1 = sample_entry(1, 10);
1679-
1680-        // Blank vault_id
1681-        assert!(db
1682-            .stage_inbox_page("", "gdrive", "c1", "c2", true, &[e1.clone()], 1000)
1683-            .is_err());
1684-
1685-        // Blank provider_id
1686-        assert!(db
1687-            .stage_inbox_page("v1", "", "c1", "c2", true, &[e1.clone()], 1000)
1688-            .is_err());
1689-
1690-        // Blank page_cursor for non-empty page
1691-        assert!(db
1692-            .stage_inbox_page("v1", "gdrive", "", "", true, &[e1.clone()], 1000)
1693-            .is_err());
1694-
1695-        // remote_seq > i64::MAX
1696-        let mut e_seq_over = e1.clone();
1697-        e_seq_over.remote_seq = Some(9_223_372_036_854_775_808u64);
1698-        assert!(db
1699-            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_seq_over], 1000)
1700-            .is_err());
1701-
1702-        // Empty remote_position
1703-        let mut e_empty_pos = e1.clone();
1704-        e_empty_pos.remote_position = "".to_string();
1705-        assert!(db
1706-            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_empty_pos], 1000)
1707-            .is_err());
1708-
1709-        // Over entry count limit
1710-        let over_entries: Vec<_> = (0..=MAX_INBOX_STAGE_ENTRIES)
1711-            .map(|i| {
1712-                let seq_val: u64 = i.try_into().unwrap();
1713-                let op_val: u8 = (i % 255).try_into().unwrap();
1714-                sample_entry(op_val, seq_val)
1715-            })
1716-            .collect();
1717-        assert!(db
1718-            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &over_entries, 1000)
1719-            .is_err());
1720-    }
1721-
1722-    #[test]
1723-    fn corrupt_operation_id_length_returns_error() {
1724-        let db = setup_test_db();
1725-        db.conn
1726-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1727-            .unwrap();
1728-
1729-        db.conn
1730-            .execute(
1731-                "INSERT INTO sync_inbox (
1732-                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
1733-                    entry_kind, state, received_at, updated_at
1734-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'pending', 100, 100)",
1735-                params![vec![99u8; 10], vec![1u8; 32]],
1736-            )
1737-            .unwrap();
1738-
1739-        let mut stmt = db
1740-            .conn
1741-            .prepare(
1742-                "SELECT vault_id, provider_id, page_cursor, remote_position, remote_seq,
1743-                        operation_id, doc_hash, entry_kind, encrypted_payload, payload_hash,
1744-                        source_device, state, last_error, received_at, updated_at, applied_at
1745-                 FROM sync_inbox",
1746-            )
1747-            .unwrap();
1748-        let res = stmt.query_row([], decode_inbox_row);
1749-        assert!(
1750-            res.is_err(),
1751-            "operation_id with 10 bytes must return Err when decoding"
1752-        );
1753-    }
1754-
1755-    #[test]
1756-    fn corrupt_doc_hash_length_returns_error() {
1757-        let db = setup_test_db();
1758-        db.conn
1759-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1760-            .unwrap();
1761-
1762-        db.conn
1763-            .execute(
1764-                "INSERT INTO sync_inbox (
1765-                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
1766-                    entry_kind, state, received_at, updated_at
1767-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'pending', 100, 100)",
1768-                params![vec![1u8; 16], vec![99u8; 10]],
1769-            )
1770-            .unwrap();
1771-
1772-        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
1773-        assert!(
1774-            res.is_err(),
1775-            "doc_hash with 10 bytes must return Err when decoding"
1776-        );
1777-    }
1778-
1779-    #[test]
1780-    fn corrupt_payload_hash_length_returns_error() {
1781-        let db = setup_test_db();
1782-        db.conn
1783-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1784-            .unwrap();
1785-
1786-        db.conn
1787-            .execute(
1788-                "INSERT INTO sync_inbox (
1789-                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
1790-                    payload_hash, entry_kind, state, received_at, updated_at
1791-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, ?3, 'upsert', 'pending', 100, 100)",
1792-                params![vec![1u8; 16], vec![2u8; 32], vec![99u8; 10]],
1793-            )
1794-            .unwrap();
1795-
1796-        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
1797-        assert!(
1798-            res.is_err(),
1799-            "payload_hash with 10 bytes must return Err when decoding"
1800-        );
1801-    }
1802-
1803-    #[test]
1804-    fn corrupt_negative_remote_seq_returns_error() {
1805-        let db = setup_test_db();
1806-        db.conn
1807-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1808-            .unwrap();
1809-
1810-        db.conn
1811-            .execute(
1812-                "INSERT INTO sync_inbox (
1813-                    vault_id, provider_id, page_cursor, remote_position, remote_seq, operation_id, doc_hash,
1814-                    entry_kind, state, received_at, updated_at
1815-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', -5, ?1, ?2, 'upsert', 'pending', 100, 100)",
1816-                params![vec![1u8; 16], vec![2u8; 32]],
1817-            )
1818-            .unwrap();
1819-
1820-        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
1821-        assert!(
1822-            res.is_err(),
1823-            "negative remote_seq must return Err when decoding"
1824-        );
1825-    }
1826-
1827-    #[test]
1828-    fn corrupt_entry_kind_returns_error() {
1829-        let db = setup_test_db();
1830-        db.conn
1831-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1832-            .unwrap();
1833-
1834-        db.conn
1835-            .execute(
1836-                "INSERT INTO sync_inbox (
1837-                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
1838-                    entry_kind, state, received_at, updated_at
1839-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'bogus_kind', 'pending', 100, 100)",
1840-                params![vec![1u8; 16], vec![2u8; 32]],
1841-            )
1842-            .unwrap();
1843-
1844-        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
1845-        assert!(
1846-            res.is_err(),
1847-            "invalid entry_kind string must return Err when decoding"
1848-        );
1849-    }
1850-
1851-    #[test]
1852-    fn corrupt_inbox_state_returns_error() {
1853-        let db = setup_test_db();
1854-        db.conn
1855-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1856-            .unwrap();
1857-
1858-        db.conn
1859-            .execute(
1860-                "INSERT INTO sync_inbox (
1861-                    vault_id, provider_id, page_cursor, remote_position, operation_id, doc_hash,
1862-                    entry_kind, state, received_at, updated_at
1863-                ) VALUES ('v1', 'gdrive', 'c1', 'p1', ?1, ?2, 'upsert', 'bogus_state', 100, 100)",
1864-                params![vec![1u8; 16], vec![2u8; 32]],
1865-            )
1866-            .unwrap();
1867-
1868-        let res = db.get_inbox_by_id("v1", "gdrive", &[1; 16]);
1869-        assert!(
1870-            res.is_err(),
1871-            "invalid state string must return Err when decoding"
1872-        );
1873-    }
1874-
1875-    #[test]
1876-    fn duplicate_against_corrupt_existing_row_rolls_back_page() {
1877-        let db = setup_test_db();
1878-        let e_corrupt_op = sample_entry(2, 20);
1879-
1880-        // Stage valid existing row first
1881-        db.stage_inbox_page(
1882-            "v1",
1883-            "gdrive",
1884-            "c1",
1885-            "c1_next",
1886-            true,
1887-            &[e_corrupt_op.clone()],
1888-            1000,
1889-        )
1890-        .unwrap();
1891-
1892-        // Turn off check constraints and corrupt the state of existing row in DB
1893-        db.conn
1894-            .execute_batch("PRAGMA ignore_check_constraints = ON;")
1895-            .unwrap();
1896-        db.conn
1897-            .execute(
1898-                "UPDATE sync_inbox SET state = 'bogus_state' WHERE vault_id = 'v1' AND provider_id = 'gdrive' AND operation_id = ?1",
1899-                params![[2u8; 16]],
1900-            )
1901-            .unwrap();
1902-
1903-        // Stage a new page: new entry A (op 1) first, duplicate of corrupt existing row second
1904-        let e1_new = sample_entry(1, 10);
1905-        let res = db.stage_inbox_page(
1906-            "v1",
1907-            "gdrive",
1908-            "c2",
1909-            "c2_next",
1910-            true,
1911-            &[e1_new, e_corrupt_op],
1912-            2000,
1913-        );
1914-
1915-        // Assert staging failed
1916-        assert!(
1917-            res.is_err(),
1918-            "staging duplicate against corrupt existing row must fail"
1919-        );
1920-
1921-        // Assert entry A was rolled back and does NOT exist in DB
1922-        assert!(
1923-            db.get_inbox_by_id("v1", "gdrive", &[1; 16])
1924-                .unwrap()
1925-                .is_none(),
1926-            "entry A must be rolled back when page staging fails due to corrupt existing row"
1927-        );
1928-    }
1929-
1930-    #[test]
1931-    fn stage_page_rejects_payload_over_byte_limit_before_write() {
1932-        let db = setup_test_db();
1933-        let mut e_over_payload = sample_entry(1, 10);
1934-        e_over_payload.encrypted_payload = Some(vec![0u8; MAX_INBOX_STAGE_PAYLOAD_BYTES + 1]);
1935-
1936-        let res = db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_over_payload], 1000);
1937-        assert!(
1938-            res.is_err(),
1939-            "staging page with payload over byte limit must return Err"
1940-        );
1941-
1942-        // Assert no partial write occurred
1943-        assert!(
1944-            db.get_inbox_by_id("v1", "gdrive", &[1; 16])
1945-                .unwrap()
1946-                .is_none(),
1947-            "operation_id must not exist in DB after over byte limit rejection"
1948-        );
1949-    }
1950-
1951-    #[test]
1952-    fn asset_reference_stages_and_reads_as_typed_kind() {
1953-        let db = setup_test_db();
1954-        let mut e_asset = sample_entry(1, 10);
1955-        e_asset.entry_kind = SyncEntryKind::AssetReference;
1956-
1957-        let res = db
1958-            .stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_asset], 1000)
1959-            .unwrap();
1960-        assert_eq!(res.inserted_count, 1);
1961-        assert_eq!(res.duplicate_count, 0);
1962-
1963-        let fetched = db
1964-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
1965-            .unwrap()
1966-            .expect("Inbox record must exist");
1967-
1968-        assert_eq!(fetched.entry_kind, SyncEntryKind::AssetReference);
1969-        assert_eq!(fetched.state, InboxState::Pending);
1970-    }
1971-
1972-    #[test]
1973-    fn candidate_query_only_returns_pending() {
1974-        let db = setup_test_db();
1975-        let e1 = sample_entry(1, 10);
1976-        let e2 = sample_entry(2, 20);
1977-        let e3 = sample_entry(3, 30);
1978-        let e4 = sample_entry(4, 40);
1979-        let e5 = sample_entry(5, 50);
1980-        let e6 = sample_entry(6, 60);
1981-        let e7 = sample_entry(7, 70);
1982-
1983-        db.stage_inbox_page(
1984-            "v1",
1985-            "gdrive",
1986-            "c1",
1987-            "c2",
1988-            true,
1989-            &[
1990-                e1.clone(),
1991-                e2.clone(),
1992-                e3.clone(),
1993-                e4.clone(),
1994-                e5.clone(),
1995-                e6.clone(),
1996-                e7.clone(),
1997-            ],
1998-            1000,
1999-        )
2000-        .unwrap();
2001-
2002-        // e2 -> Applying
2003-        db.transition_inbox_state(
2004-            "v1",
2005-            "gdrive",
2006-            &[2; 16],
2007-            InboxState::Pending,
2008-            InboxState::Applying,
2009-            None,
2010-            1100,
2011-        )
2012-        .unwrap();
2013-
2014-        // e3 -> PendingAsset
2015-        db.transition_inbox_state(
2016-            "v1",
2017-            "gdrive",
2018-            &[3; 16],
2019-            InboxState::Pending,
2020-            InboxState::Applying,
2021-            None,
2022-            1100,
2023-        )
2024-        .unwrap();
2025-        db.transition_inbox_state(
2026-            "v1",
2027-            "gdrive",
2028-            &[3; 16],
2029-            InboxState::Applying,
2030-            InboxState::PendingAsset,
2031-            None,
2032-            1200,
2033-        )
2034-        .unwrap();
2035-
2036-        // e4 -> Applied
2037-        db.transition_inbox_state(
2038-            "v1",
2039-            "gdrive",
2040-            &[4; 16],
2041-            InboxState::Pending,
2042-            InboxState::Applying,
2043-            None,
2044-            1100,
2045-        )
2046-        .unwrap();
2047-        db.transition_inbox_state(
2048-            "v1",
2049-            "gdrive",
2050-            &[4; 16],
2051-            InboxState::Applying,
2052-            InboxState::Applied,
2053-            None,
2054-            1200,
2055-        )
2056-        .unwrap();
2057-
2058-        // e5 -> IgnoredOwnOperation
2059-        db.transition_inbox_state(
2060-            "v1",
2061-            "gdrive",
2062-            &[5; 16],
2063-            InboxState::Pending,
2064-            InboxState::Applying,
2065-            None,
2066-            1100,
2067-        )
2068-        .unwrap();
2069-        db.transition_inbox_state(
2070-            "v1",
2071-            "gdrive",
2072-            &[5; 16],
2073-            InboxState::Applying,
2074-            InboxState::IgnoredOwnOperation,
2075-            None,
2076-            1200,
2077-        )
2078-        .unwrap();
2079-
2080-        // e6 -> Failed
2081-        db.transition_inbox_state(
2082-            "v1",
2083-            "gdrive",
2084-            &[6; 16],
2085-            InboxState::Pending,
2086-            InboxState::Applying,
2087-            None,
2088-            1100,
2089-        )
2090-        .unwrap();
2091-        db.transition_inbox_state(
2092-            "v1",
2093-            "gdrive",
2094-            &[6; 16],
2095-            InboxState::Applying,
2096-            InboxState::Failed,
2097-            Some("error msg"),
2098-            1200,
2099-        )
2100-        .unwrap();
2101-
2102-        // e7 -> Quarantined
2103-        db.transition_inbox_state(
2104-            "v1",
2105-            "gdrive",
2106-            &[7; 16],
2107-            InboxState::Pending,
2108-            InboxState::Applying,
2109-            None,
2110-            1100,
2111-        )
2112-        .unwrap();
2113-        db.transition_inbox_state(
2114-            "v1",
2115-            "gdrive",
2116-            &[7; 16],
2117-            InboxState::Applying,
2118-            InboxState::Quarantined,
2119-            Some("quarantine msg"),
2120-            1200,
2121-        )
2122-        .unwrap();
2123-
2124-        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
2125-        assert_eq!(candidates.len(), 1);
2126-        assert_eq!(candidates[0].operation_id, [1; 16]);
2127-        assert_eq!(candidates[0].state, InboxState::Pending);
2128-
2129-        let non_pending_op_ids: Vec<[u8; 16]> =
2130-            vec![[2; 16], [3; 16], [4; 16], [5; 16], [6; 16], [7; 16]];
2131-        for op_id in non_pending_op_ids {
2132-            assert!(
2133-                !candidates.iter().any(|c| c.operation_id == op_id),
2134-                "Candidates list must not contain non-pending operation_id hex '{}'",
2135-                hex::encode(op_id)
2136-            );
2137-        }
2138-    }
2139-
2140-    #[test]
2141-    fn candidate_query_and_cas_vault_and_provider_isolation() {
2142-        let db = setup_test_db();
2143-        db.conn
2144-            .execute(
2145-                "INSERT INTO sync_vaults (vault_id, canonical_root, created_at, updated_at) VALUES ('v2', '/v2', 200, 200)",
2146-                [],
2147-            )
2148-            .unwrap();
2149-        db.conn
2150-            .execute(
2151-                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v1', 'server', 100, 100)",
2152-                [],
2153-            )
2154-            .unwrap();
2155-        db.conn
2156-            .execute(
2157-                "INSERT INTO sync_provider_state (vault_id, provider_id, created_at, updated_at) VALUES ('v2', 'gdrive', 200, 200)",
2158-                [],
2159-            )
2160-            .unwrap();
2161-
2162-        let mut e_v1 = sample_entry(1, 10);
2163-        let mut e_v2 = sample_entry(1, 10); // SAME operation_id [1; 16] in both vaults!
2164-        e_v1.source_device = Some("device_v1".to_string());
2165-        e_v2.source_device = Some("device_v2".to_string());
2166-
2167-        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_v1.clone()], 1000)
2168-            .unwrap();
2169-        db.stage_inbox_page("v2", "gdrive", "c2", "c3", true, &[e_v2.clone()], 2000)
2170-            .unwrap();
2171-
2172-        // Candidates for v1/gdrive
2173-        let cand_v1 = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
2174-        assert_eq!(cand_v1.len(), 1);
2175-        assert_eq!(cand_v1[0].vault_id, "v1");
2176-        assert_eq!(cand_v1[0].provider_id, "gdrive");
2177-        assert_eq!(cand_v1[0].source_device, Some("device_v1".to_string()));
2178-
2179-        // Candidates for v2/gdrive
2180-        let cand_v2 = db.get_inbox_apply_candidates("v2", "gdrive", 10).unwrap();
2181-        assert_eq!(cand_v2.len(), 1);
2182-        assert_eq!(cand_v2[0].vault_id, "v2");
2183-        assert_eq!(cand_v2[0].provider_id, "gdrive");
2184-        assert_eq!(cand_v2[0].source_device, Some("device_v2".to_string()));
2185-
2186-        // CAS transition v1/gdrive record to Applying
2187-        let v2_before = db
2188-            .get_inbox_by_id("v2", "gdrive", &[1; 16])
2189-            .unwrap()
2190-            .unwrap();
2191-        db.transition_inbox_state(
2192-            "v1",
2193-            "gdrive",
2194-            &[1; 16],
2195-            InboxState::Pending,
2196-            InboxState::Applying,
2197-            None,
2198-            1500,
2199-        )
2200-        .unwrap();
2201-
2202-        // Verify v1 changed
2203-        let v1_after = db
2204-            .get_inbox_by_id("v1", "gdrive", &[1; 16])
2205-            .unwrap()
2206-            .unwrap();
2207-        assert_eq!(v1_after.state, InboxState::Applying);
2208-        assert_eq!(v1_after.updated_at, 1500);
2209-
2210-        // Verify v2 remains UNCHANGED
2211-        let v2_after = db
2212-            .get_inbox_by_id("v2", "gdrive", &[1; 16])
2213-            .unwrap()
2214-            .unwrap();
2215-        assert_eq!(v2_after, v2_before);
2216-
2217-        // CAS with wrong vault/provider returns Err
2218-        assert!(db
2219-            .transition_inbox_state(
2220-                "v1",
2221-                "non_existent_provider",
2222-                &[1; 16],
2223-                InboxState::Pending,
2224-                InboxState::Applying,
2225-                None,
2226-                1600,
2227-            )
2228-            .is_err());
2229-        assert!(db
2230-            .transition_inbox_state(
2231-                "non_existent_vault",
2232-                "gdrive",
2233-                &[1; 16],
2234-                InboxState::Pending,
2235-                InboxState::Applying,
2236-                None,
2237-                1600,
2238-            )
2239-            .is_err());
2240-    }
2241-
2242-    #[test]
2243-    fn candidate_query_null_seq_ordering_regression() {
2244-        let db = setup_test_db();
2245-        let mut e_z = sample_entry(26, 0); // byte 26 => op_id [26; 16]
2246-        let mut e_a = sample_entry(1, 0); // byte 1 => op_id [1; 16]
2247-
2248-        e_z.remote_seq = None;
2249-        e_a.remote_seq = None;
2250-
2251-        // Equal received_at and page_cursor and remote_position
2252-        e_z.remote_position = "pos_same".to_string();
2253-        e_a.remote_position = "pos_same".to_string();
2254-
2255-        // Stage in REVERSE order (e_z first, e_a second)
2256-        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e_z, e_a], 1000)
2257-            .unwrap();
2258-
2259-        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
2260-        assert_eq!(candidates.len(), 2);
2261-        assert_eq!(candidates[0].remote_seq, None);
2262-        assert_eq!(candidates[1].remote_seq, None);
2263-
2264-        // Tie-breaker on operation_id ASC: [1; 16] before [26; 16]
2265-        assert_eq!(candidates[0].operation_id, [1; 16]);
2266-        assert_eq!(candidates[1].operation_id, [26; 16]);
2267-    }
2268-
2269-    #[test]
2270-    fn candidate_query_bounded_by_limit() {
2271-        let db = setup_test_db();
2272-        let e1 = sample_entry(1, 10);
2273-        let e2 = sample_entry(2, 20);
2274-        let e3 = sample_entry(3, 30);
2275-
2276-        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1, e2, e3], 1000)
2277-            .unwrap();
2278-
2279-        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 2).unwrap();
2280-        assert_eq!(candidates.len(), 2);
2281-    }
2282-
2283-    #[test]
2284-    fn candidate_query_sorts_numeric_seq_asc() {
2285-        let db = setup_test_db();
2286-        let e10 = sample_entry(1, 10);
2287-        let e2 = sample_entry(2, 2);
2288-        let e1 = sample_entry(3, 1);
2289-
2290-        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e10], 1000)
2291-            .unwrap();
2292-        db.stage_inbox_page("v1", "gdrive", "c2", "c3", true, &[e2], 1000)
2293-            .unwrap();
2294-        db.stage_inbox_page("v1", "gdrive", "c3", "c4", true, &[e1], 1000)
2295-            .unwrap();
2296-
2297-        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
2298-        assert_eq!(candidates.len(), 3);
2299-        assert_eq!(candidates[0].remote_seq, Some(1));
2300-        assert_eq!(candidates[1].remote_seq, Some(2));
2301-        assert_eq!(candidates[2].remote_seq, Some(10));
2302-    }
2303-
2304-    #[test]
2305-    fn candidate_query_deterministic_tie_breaker() {
2306-        let db = setup_test_db();
2307-        let mut e1 = sample_entry(1, 5);
2308-        let mut e2 = sample_entry(2, 5);
2309-        e1.remote_seq = None;
2310-        e2.remote_seq = None;
2311-        e1.remote_position = "pos_a".to_string();
2312-        e2.remote_position = "pos_b".to_string();
2313-
2314-        db.stage_inbox_page("v1", "gdrive", "c1", "c2", true, &[e1, e2], 1000)
2315-            .unwrap();
2316-
2317-        let candidates = db.get_inbox_apply_candidates("v1", "gdrive", 10).unwrap();
2318-        assert_eq!(candidates.len(), 2);
2319-        assert_eq!(candidates[0].remote_position, "pos_a");
2320-        assert_eq!(candidates[1].remote_position, "pos_b");
2321-    }

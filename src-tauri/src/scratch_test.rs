pub fn foo() {
    let app = tauri::test::mock_builder().build();
    let handle = app.handle();
}

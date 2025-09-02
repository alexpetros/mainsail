const SAILBOAT_DEFAULT_DB: &str = "sailboat.db";

fn main() {
    tracing_subscriber::fmt().init();
    let _ = std::fs::remove_file(SAILBOAT_DEFAULT_DB);

    let db = mainsail::database::Database::new(SAILBOAT_DEFAULT_DB).unwrap();
    db.init().unwrap();

    let basic_data = include_str!("../database/tests/basic-data.sql");
    db.execute_batch(basic_data).unwrap();
}


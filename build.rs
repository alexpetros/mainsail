use std::{env, error::Error, fs::{self, DirEntry, File}, io::Write, path::Path};

const MIGRATIONS_DIR: &str = "./src/database/migrations";

fn main() -> Result<(), Box<dyn Error>> {
    // Embed migration scripts
    let out_dir = env::var("OUT_DIR")?;
    let dest_path = Path::new(&out_dir).join("migrations.rs");
    let mut migrations = File::create(&dest_path)?;

    writeln!(&mut migrations, r##"["##,)?;

    // Add the migration files to the build
    let mut migration_files: Vec<DirEntry> = fs::read_dir(MIGRATIONS_DIR)
        .expect("build.rs - migrations directory not found")
        .filter_map(|f| f.ok())
        .filter(|f| { f.path().extension().unwrap() == "sql" })
        .collect();
    migration_files.sort_by_key(get_num);

    for f in migration_files {
        let path = f.path().canonicalize()?;

        writeln!(
            &mut migrations,
            r##"("{name}", include_str!(r#"{path}"#)),"##,
            name = f.file_name().into_string().unwrap(),
            path = path.to_str().unwrap()
        )?;
    }

    writeln!(&mut migrations, r##"]"##,)?;

    println!("cargo::rerun-if-changed={MIGRATIONS_DIR}");
    Ok(())
}

fn get_num(entry: &DirEntry) -> usize {
    let name = entry.file_name().into_string().expect("Unable to convert file name to string");
    let num = name.split_once("-").expect("Missing - in filename").0.trim();
    num.parse().expect("Unable to parse file into string")
}

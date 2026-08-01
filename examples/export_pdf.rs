use std::{error::Error, io, path::Path};

use typst_layout::PagedDocument;
use typst_pdf::PdfOptions;
use typstation::world::TypstationWorld;

const DEMO: &str = include_str!("../src/demo.typ");
const OUT_DIR: &str = "out";

fn main() -> Result<(), Box<dyn Error>> {
    let root = std::env::current_dir()?;
    let mut world = TypstationWorld::new(root);
    world.set_source(DEMO);

    let result = typst::compile::<PagedDocument>(&world);

    for warning in &result.warnings {
        eprintln!("aviso: {}", warning.message);
    }

    let document = match result.output {
        Ok(document) => document,
        Err(errors) => {
            for error in &errors {
                eprintln!("erro: {}", error.message);
            }

            return Err(
                io::Error::other(format!("falha ao compilar: {} erro(s)", errors.len())).into(),
            );
        }
    };

    let pdf = typst_pdf::pdf(&document, &PdfOptions::default()).map_err(|errors| {
        io::Error::other(format!("falha ao exportar PDF: {} erro(s)", errors.len()))
    })?;

    std::fs::create_dir_all(OUT_DIR)?;
    let output = Path::new(OUT_DIR).join("tutorial.pdf");
    std::fs::write(&output, pdf)?;

    println!("PDF gerado em {}", output.display());
    Ok(())
}

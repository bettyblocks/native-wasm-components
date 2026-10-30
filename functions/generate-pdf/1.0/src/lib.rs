mod pdf;
mod template;

pub mod bindings {
    wit_bindgen::generate!({
        generate_all,
    });
}

use base64::{Engine as _, engine::general_purpose::STANDARD};
use bindings::{
    betty_blocks_types::data_api::data_api::HelperContext,
    betty_blocks_types::upload_file::upload_file,
    exports::betty_blocks::generate_pdf::generate_pdf::{
        BettyMap, BettyModel, BettyProperty, Guest,
    },
};

struct Component;

impl Guest for Component {
    fn generate_pdf(
        helper_context: HelperContext,
        html: String,
        variables: Option<BettyMap>,
        model: BettyModel,
        property: Vec<BettyProperty>,
        file_name: String,
    ) -> Result<String, String> {
        let property = property
            .into_iter()
            .next()
            .ok_or("Select a property to store the PDF in")?;
        let full_filename = pdf_file_name(&file_name)?;
        let variables: Vec<(String, Option<String>)> = variables
            .unwrap_or_default()
            .into_iter()
            .map(|kv| (kv.key, kv.value))
            .collect();

        let html = template::render(&html, &variables)?;
        let pdf = pdf::render_html(&html)?;
        let uploaded = upload_file::upload(
            &helper_context,
            &upload_file::Input {
                model,
                property,
                file_base64: STANDARD.encode(pdf),
                full_filename,
            },
        )
        .map_err(|e| format!("Upload failed: {e}"))?;
        Ok(uploaded.reference)
    }
}

fn pdf_file_name(file_name: &str) -> Result<String, String> {
    let file_name = file_name.trim();
    if file_name.is_empty() {
        return Err("File name is required".to_string());
    }
    Ok(if file_name.ends_with(".pdf") {
        file_name.to_string()
    } else {
        format!("{file_name}.pdf")
    })
}

bindings::export!(Component with_types_in bindings);

#[cfg(test)]
mod tests {
    use super::pdf_file_name;

    #[test]
    fn appends_pdf_only_when_missing() {
        assert_eq!(pdf_file_name("invoice").unwrap(), "invoice.pdf");
        assert_eq!(pdf_file_name("invoice.pdf").unwrap(), "invoice.pdf");
        assert_eq!(pdf_file_name("report.PDF").unwrap(), "report.PDF.pdf");
        assert!(pdf_file_name("  ").is_err());
    }
}

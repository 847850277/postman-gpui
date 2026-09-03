use postman_http::{
    request::{MultipartPart, MultipartValue},
    HttpError,
};
use reqwest::multipart::{Form, Part};

pub(crate) async fn build_multipart(parts: &[MultipartPart]) -> Result<Form, HttpError> {
    let mut form = Form::new();

    for part in parts {
        form = match &part.value {
            MultipartValue::Text(value) => form.text(part.name.clone(), value.clone()),
            MultipartValue::File {
                path,
                file_name,
                content_type,
            } => {
                let bytes = tokio::fs::read(path).await.map_err(|error| {
                    HttpError::invalid_request(format!(
                        "failed to read multipart file for field `{}` at {}: {error}",
                        part.name,
                        path.display()
                    ))
                })?;
                let inferred_name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "upload.bin".to_owned());
                let mut file_part =
                    Part::bytes(bytes).file_name(file_name.clone().unwrap_or(inferred_name));

                if let Some(content_type) = content_type {
                    file_part = file_part.mime_str(content_type).map_err(|error| {
                        HttpError::invalid_request(format!(
                            "invalid multipart content type for field `{}`: {error}",
                            part.name
                        ))
                    })?;
                }

                form.part(part.name.clone(), file_part)
            }
        };
    }

    Ok(form)
}

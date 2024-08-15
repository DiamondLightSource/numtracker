use crate::{PathTemplateBackend, ScanNumberBackend};

#[derive(Clone)]
pub struct FallbackScanNumbering<B, FB> {
    pub primary: B,
    pub secondary: FB,
}

impl<Backend: PathTemplateBackend, Fallback: Clone + Send + Sync> PathTemplateBackend
    for FallbackScanNumbering<Backend, Fallback>
{
    type TemplateErr = Backend::TemplateErr;

    fn visit_directory_template(
        &self,
        beamline: &str,
    ) -> impl std::future::Future<Output = Result<crate::paths::VisitTemplate, Self::TemplateErr>> + Send
    {
        self.primary.visit_directory_template(beamline)
    }

    fn scan_file_template(
        &self,
        beamline: &str,
    ) -> impl std::future::Future<Output = Result<crate::paths::ScanTemplate, Self::TemplateErr>> + Send
    {
        self.primary.scan_file_template(beamline)
    }

    fn detector_file_template(
        &self,
        bl: &str,
    ) -> impl std::future::Future<Output = Result<crate::paths::DetectorTemplate, Self::TemplateErr>>
           + Send {
        self.primary.detector_file_template(bl)
    }
}

impl<Backend: ScanNumberBackend, Fallback: ScanNumberBackend> ScanNumberBackend
    for FallbackScanNumbering<Backend, Fallback>
{
    type NumberError = Backend::NumberError;

    async fn next_scan_number(&self, beamline: &str) -> Result<usize, Self::NumberError> {
        let primary = self.primary.next_scan_number(beamline).await?;
        match self.secondary.next_scan_number(beamline).await {
            Ok(num) if num == primary => (), // numbers agree
            Ok(num) => eprintln!("Fallback numbering mismatch: expected {primary}, found {num}"),
            Err(e) => eprintln!("Couldn't increment fallback scan number: {e}"),
        }
        Ok(primary)
    }
}

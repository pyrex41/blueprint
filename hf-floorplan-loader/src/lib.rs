use csv::ReaderBuilder;
use image::{DynamicImage, ImageError};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Represents a single floorplan with its metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloorplanData {
    pub file_name: String,
    pub image_path: PathBuf,
    pub description: String,
}

/// Represents a row in the metadata.csv file
#[derive(Debug, Deserialize)]
struct MetadataRow {
    file_name: String,
    text: String,
}

/// Error types for the dataset loader
#[derive(Debug)]
pub enum LoaderError {
    IoError(std::io::Error),
    CsvError(csv::Error),
    ImageError(ImageError),
    DatasetNotFound(String),
    InvalidPath(String),
    EnvironmentError(String),
}

impl std::fmt::Display for LoaderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoaderError::IoError(e) => write!(f, "IO error: {}", e),
            LoaderError::CsvError(e) => write!(f, "CSV parsing error: {}", e),
            LoaderError::ImageError(e) => write!(f, "Image loading error: {}", e),
            LoaderError::DatasetNotFound(msg) => write!(f, "Dataset not found: {}", msg),
            LoaderError::InvalidPath(msg) => write!(f, "Invalid path: {}", msg),
            LoaderError::EnvironmentError(msg) => write!(f, "Environment error: {}", msg),
        }
    }
}

impl std::error::Error for LoaderError {}

impl From<std::io::Error> for LoaderError {
    fn from(err: std::io::Error) -> Self {
        LoaderError::IoError(err)
    }
}

impl From<csv::Error> for LoaderError {
    fn from(err: csv::Error) -> Self {
        LoaderError::CsvError(err)
    }
}

impl From<ImageError> for LoaderError {
    fn from(err: ImageError) -> Self {
        LoaderError::ImageError(err)
    }
}

/// Finds the HuggingFace dataset snapshot directory
pub fn find_dataset_path() -> Result<PathBuf, LoaderError> {
    let home = std::env::var("HOME").map_err(|e| {
        LoaderError::EnvironmentError(format!("HOME environment variable not set: {}", e))
    })?;

    let base_path = PathBuf::from(&home)
        .join(".cache/huggingface/hub/datasets--umesh16071973--New_Floorplan_demo_dataset");

    if !base_path.exists() {
        return Err(LoaderError::DatasetNotFound(format!(
            "Dataset directory not found at: {}",
            base_path.display()
        )));
    }

    // Find the snapshots directory
    let snapshots_dir = base_path.join("snapshots");
    if !snapshots_dir.exists() {
        return Err(LoaderError::DatasetNotFound(format!(
            "Snapshots directory not found at: {}",
            snapshots_dir.display()
        )));
    }

    // Get the first (and typically only) snapshot directory
    let mut snapshot_dirs: Vec<_> = fs::read_dir(&snapshots_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().is_dir())
        .collect();

    if snapshot_dirs.is_empty() {
        return Err(LoaderError::DatasetNotFound(format!(
            "No snapshot directories found in: {}",
            snapshots_dir.display()
        )));
    }

    snapshot_dirs.sort_by_key(|dir| dir.path());
    Ok(snapshot_dirs[0].path())
}

/// Parse the metadata.csv file and return FloorplanData entries
pub fn parse_metadata(dataset_path: &Path) -> Result<Vec<FloorplanData>, LoaderError> {
    let metadata_path = dataset_path.join("metadata.csv");

    if !metadata_path.exists() {
        return Err(LoaderError::InvalidPath(format!(
            "metadata.csv not found at: {}",
            metadata_path.display()
        )));
    }

    let mut reader = ReaderBuilder::new()
        .has_headers(true)
        .from_path(&metadata_path)?;

    let mut floorplans = Vec::new();

    for result in reader.deserialize() {
        let record: MetadataRow = result?;
        let image_path = dataset_path.join(&record.file_name);

        floorplans.push(FloorplanData {
            file_name: record.file_name,
            image_path,
            description: record.text,
        });
    }

    Ok(floorplans)
}

/// Load a floorplan image from disk
pub fn load_floorplan_image(path: &Path) -> Result<DynamicImage, LoaderError> {
    Ok(image::open(path)?)
}

/// Validate that all images in the dataset are accessible
pub fn validate_images(floorplans: &[FloorplanData]) -> Result<Vec<String>, LoaderError> {
    let mut errors = Vec::new();

    for floorplan in floorplans {
        if !floorplan.image_path.exists() {
            errors.push(format!("Missing image: {}", floorplan.file_name));
            continue;
        }

        if let Err(e) = load_floorplan_image(&floorplan.image_path) {
            errors.push(format!("Failed to load {}: {:?}", floorplan.file_name, e));
        }
    }

    Ok(errors)
}

/// Dataset iterator with batch loading and shuffling support
pub struct FloorplanDataset {
    floorplans: Vec<FloorplanData>,
    current_index: usize,
}

impl FloorplanDataset {
    /// Create a new dataset from the HuggingFace cache
    pub fn new() -> Result<Self, LoaderError> {
        let dataset_path = find_dataset_path()?;
        let floorplans = parse_metadata(&dataset_path)?;

        Ok(Self {
            floorplans,
            current_index: 0,
        })
    }

    /// Create a dataset from a custom path
    pub fn from_path(path: &Path) -> Result<Self, LoaderError> {
        let floorplans = parse_metadata(path)?;

        Ok(Self {
            floorplans,
            current_index: 0,
        })
    }

    /// Get the total number of floorplans
    pub fn len(&self) -> usize {
        self.floorplans.len()
    }

    /// Check if dataset is empty
    pub fn is_empty(&self) -> bool {
        self.floorplans.is_empty()
    }

    /// Shuffle the dataset order
    pub fn shuffle(&mut self) {
        use std::collections::hash_map::RandomState;
        use std::hash::{BuildHasher, Hash, Hasher};

        // Simple Fisher-Yates shuffle using a basic RNG
        let mut rng_state = RandomState::new().build_hasher().finish();

        for i in (1..self.floorplans.len()).rev() {
            // Simple linear congruential generator
            rng_state = rng_state.wrapping_mul(6364136223846793005).wrapping_add(1);
            let j = (rng_state as usize) % (i + 1);
            self.floorplans.swap(i, j);
        }

        self.current_index = 0;
    }

    /// Get a batch of floorplans
    pub fn batch(&mut self, size: usize) -> Vec<FloorplanData> {
        let end = (self.current_index + size).min(self.floorplans.len());
        let batch: Vec<FloorplanData> = self.floorplans[self.current_index..end].to_vec();
        self.current_index = end;
        batch
    }

    /// Split dataset into train/val/test sets
    pub fn split(&self, train_ratio: f64, val_ratio: f64) -> (Vec<FloorplanData>, Vec<FloorplanData>, Vec<FloorplanData>) {
        let total = self.floorplans.len();
        let train_size = (total as f64 * train_ratio) as usize;
        let val_size = (total as f64 * val_ratio) as usize;

        let train = self.floorplans[..train_size].to_vec();
        let val = self.floorplans[train_size..train_size + val_size].to_vec();
        let test = self.floorplans[train_size + val_size..].to_vec();

        (train, val, test)
    }

    /// Reset iterator to beginning
    pub fn reset(&mut self) {
        self.current_index = 0;
    }

    /// Get all floorplans
    pub fn all(&self) -> &[FloorplanData] {
        &self.floorplans
    }
}

impl Iterator for FloorplanDataset {
    type Item = FloorplanData;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_index < self.floorplans.len() {
            let item = self.floorplans[self.current_index].clone();
            self.current_index += 1;
            Some(item)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_floorplan_data_creation() {
        let data = FloorplanData {
            file_name: "0.jpg".to_string(),
            image_path: PathBuf::from("/path/to/0.jpg"),
            description: "A 3 room apartment".to_string(),
        };

        assert_eq!(data.file_name, "0.jpg");
        assert_eq!(data.description, "A 3 room apartment");
    }

    #[test]
    fn test_dataset_split() {
        // Create a mock dataset
        let floorplans: Vec<FloorplanData> = (0..10)
            .map(|i| FloorplanData {
                file_name: format!("{}.jpg", i),
                image_path: PathBuf::from(format!("/path/{}.jpg", i)),
                description: format!("Room {}", i),
            })
            .collect();

        let dataset = FloorplanDataset {
            floorplans,
            current_index: 0,
        };

        let (train, val, test) = dataset.split(0.8, 0.1);

        assert_eq!(train.len(), 8);
        assert_eq!(val.len(), 1);
        assert_eq!(test.len(), 1);
    }

    #[test]
    fn test_batch_loading() {
        let floorplans: Vec<FloorplanData> = (0..10)
            .map(|i| FloorplanData {
                file_name: format!("{}.jpg", i),
                image_path: PathBuf::from(format!("/path/{}.jpg", i)),
                description: format!("Room {}", i),
            })
            .collect();

        let mut dataset = FloorplanDataset {
            floorplans,
            current_index: 0,
        };

        let batch1 = dataset.batch(3);
        assert_eq!(batch1.len(), 3);
        assert_eq!(batch1[0].file_name, "0.jpg");

        let batch2 = dataset.batch(3);
        assert_eq!(batch2.len(), 3);
        assert_eq!(batch2[0].file_name, "3.jpg");
    }

    #[test]
    fn test_iterator() {
        let floorplans: Vec<FloorplanData> = (0..5)
            .map(|i| FloorplanData {
                file_name: format!("{}.jpg", i),
                image_path: PathBuf::from(format!("/path/{}.jpg", i)),
                description: format!("Room {}", i),
            })
            .collect();

        let mut dataset = FloorplanDataset {
            floorplans,
            current_index: 0,
        };

        let collected: Vec<_> = dataset.take(3).collect();
        assert_eq!(collected.len(), 3);
        assert_eq!(collected[0].file_name, "0.jpg");
        assert_eq!(collected[2].file_name, "2.jpg");
    }
}

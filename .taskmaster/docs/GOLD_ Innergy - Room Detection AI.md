# **Product Requirements Document (PRD): Location Detection AI**

## **1\. Introduction and Goal**

### **1.1 Project Goal**

The primary goal of the Location Detection AI project is to drastically reduce the manual effort required by users to define "locations" (rooms, hallways, etc.) on architectural blueprints. We aim to build an AI service capable of automatically detecting and outputting the boundaries of distinct rooms from a blueprint image or vector file.

### **1.2 Context**

Innergy users currently spend a significant amount of time manually tracing room boundaries using 2D CAD tools. Automating this step is a critical feature for improving user experience and is expected to be a major selling point for our platform. We previously attempted to outsource this functionality, but the resulting solution was inadequate, necessitating an in-house, robust development effort.

## **2\. Problem & Business Context**

### **2.1 Problem Statement**

Users waste a great deal of time drawing room boundaries (often rectangular but sometimes arbitrary shapes) on architectural blueprints. We need an Artificial Intelligence solution that can analyze a blueprint image or vector file and identify the precise boundaries of individual rooms, automating the creation of these "location" objects.

### **2.2 Current State & Opportunity**

Currently, we have an internal AI tool that successfully extracts the room name and number **after** the user manually draws the boundary. The missing piece is the boundary-drawing step itself. Automating this drawing process will save significant user clicking and setup time, transforming a tedious task into an instant process.

### **2.3 Success Metrics (Impact)**

The success of this project will be measured by two key factors:

1. **User Efficiency:** Save a great deal of user clicking and manual labor (e.g., reducing the time to map a 10-room floor plan from 5 minutes to under 30 seconds).  
2. **Sales & Market Appeal:** The tool is highly attractive for sales, serving as a powerful competitive differentiator.

## **3\. Proposed Solution: The Location Detection Service**

We propose building a dedicated, server-side AI service that acts as a blueprint processing pipeline.

### **3.1 Core Functional Requirement**

The service **MUST** be able to:

1. Accept a blueprint file (image format like PNG/JPG, or the existing PDF vector data).  
2. Process the file using AI/ML models.  
3. Return the coordinates of all detected "rooms" on the blueprint.

The returned coordinates must define either the rectangular bounding box or the precise shape vertices of the detected room.

### **3.2 System Flow (High-Level)**

1. User uploads a Blueprint image to the front-end application (React).  
2. The application sends the file to the **AWS-hosted** Location Detection Service.  
3. The Service processes the image/data, utilizing **AWS AI/ML Services** (e.g., Amazon Textract, Amazon SageMaker, AWS Computer Vision services) or other necessary AI services as required.  
4. The Service returns a JSON object containing the coordinates of the detected rooms.  
5. The React front-end renders the automatically-created room boundaries on the blueprint visualization.

## **4\. Technical Requirements and Constraints**

### **4.1 Technical Stack**

* **Cloud Platform:** **AWS** (Mandatory)  
* **AI Frameworks:** **AWS AI/ML Services** (e.g., Amazon Textract, Amazon SageMaker, AWS Computer Vision). The solution must integrate the necessary document processing capabilities (similar to DocumentAI).  
* **Development Tools:** React (Front-end), Visual Studio Code / Visual Studio (Back-end logic).

### **4.2 Performance Benchmarks**

* **Latency:** Processing time **MUST be less than 30 seconds per blueprint**.

### **4.3 Off-Limits Technology**

* The solution must rely on established engineering principles. Any reliance on "Magic" is strictly forbidden.

## **5\. Mock Data Strategy for Students**

To allow students to develop and test the core logic without access to proprietary Innergy blueprints, the project will use a simplified mock data structure and public domain sample blueprints.

### **5.1 Input Mock Data (Simulated Blueprint)**

Students should target a simplified, generic floor plan image (public domain or placeholder) for visual testing.

**Format for Mock Blueprint Input:** Instead of a complex PDF vector, the input to the AI model development can be simplified to a **raster image** and a corresponding JSON array representing the key structural lines (walls) in normalized coordinates (0-1000).

```
[
  // Represents a horizontal wall segment
  {"type": "line", "start": [100, 100], "end": [500, 100], "is_load_bearing": false},
  // Represents a vertical wall segment
  {"type": "line", "start": [100, 100], "end": [100, 400], "is_load_bearing": false},
  // ... more lines defining a few simple rooms
]

```

### **5.2 Expected Output Mock Data (Detected Rooms)**

The service must return a JSON array containing the identified room boundaries.

**Output Schema: `DetectedRoom`** The primary output should be a list of room boundaries, defined by a simple bounding box for the initial MVP.

| Field | Type | Description |
| ----- | ----- | ----- |
| `id` | String | Unique identifier for the room. |
| `bounding_box` | Array | Normalized coordinates: `[x_min, y_min, x_max, y_max]` (0-1000 range). |
| `name_hint` | String | Optional: A hint for the name (e.g., "Kitchen", "Office") for training/debugging purposes. |

```
[
  {
    "id": "room_001",
    "bounding_box": [50, 50, 200, 300],
    "name_hint": "Entry Hall"
  },
  {
    "id": "room_002",
    "bounding_box": [250, 50, 700, 500],
    "name_hint": "Main Office"
  }
  // ... more rooms
]

```

## **6\. Project Deliverables**

### **6.1 Submission Requirements**

Successful completion of the Gauntlet AI project requires the following submissions:

1. **Code Repository:** Fully functional code hosted on a platform (GitHub preferred).  
2. **Demo:** A video or live presentation demonstrating the working service (React front-end submitting the mock blueprint and receiving/displaying the coordinates).  
3. **Brief Technical Writeup (1-2 pages):** Documenting the methodology, model choices, and data preparation process.  
4. **AI Documentation:** Detailed documentation of the specific **AWS AI/ML services** and configuration settings used.


from pathlib import Path
import os
import glob

from django.core.files.uploadedfile import UploadedFile

from pypdf import PdfReader

SHIPMENT_UPLOAD_FOLDER = "uploads/"


class ShipmentFileHandler:
    def __init__(self, folder_path: str):
        self.folder_path = folder_path

    def _get_next_name(self) -> str:
        # Find all files in the folder that match the pattern
        files = glob.glob(os.path.join(self.folder_path, "shipment_*.pdf"))
        
        # Extract numbers from filenames and find the maximum
        max_number = 0
        for file in files:
            base_name = os.path.basename(file)
            if base_name.startswith("shipment_") and base_name.endswith(".pdf"):
                try:
                    number = int(base_name[len("shipment_"):-len(".pdf")])
                    max_number = max(max_number, number)
                except ValueError:
                    continue
        
        # Return the next filename
        return os.path.join(self.folder_path, f"shipment_{max_number + 1}.pdf")
    
    def handle_file(self, file : UploadedFile) -> tuple[Path, str] | None:
        """
        Handle a file from a form, save it and return its (Path, content).
        """
        if not os.path.exists(self.folder_path):
            os.makedirs(self.folder_path)

        file_path = self._get_next_name()
        try:
            with open(file_path, 'wb') as f:
                for chunk in file.chunks():
                    f.write(chunk)

            
            return (Path(file_path), PdfReader(file_path).pages[0].extract_text())
        except Exception as e:
            print(f"An error occurred while saving the file: {e}")
            return None
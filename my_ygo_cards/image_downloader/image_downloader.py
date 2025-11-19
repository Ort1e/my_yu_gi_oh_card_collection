import requests
import os
from django.core.files.base import ContentFile
from django.core.files.storage import default_storage

from yu_gi_oh_collection.settings import BASE_DIR, MEDIA_ROOT, MEDIA_URL


IMAGE_FOLDER = "card_images/"
API_URL = "https://db.ygoprodeck.com/api/v7/cardinfo.php"

class ImageDownloader:
    @staticmethod
    def clean_card_name(card_name: str) -> str:
        """
        Clean the card name by removing unwanted parentheses and lowercasing.
        """
        if '(' in card_name:
            card_name = card_name.split('(')[0].strip()
        return card_name.lower()

    def __init__(self, card_name: str):
        cleaned_name = self.clean_card_name(card_name)

        self.card_name = cleaned_name
        path = IMAGE_FOLDER + cleaned_name.replace(" ", "_").lower() + ".jpg"

        if not os.path.exists(MEDIA_ROOT / path):
            path = self._download(cleaned_name)
            if path is None:
                self.image_path = ""

        self.image_path = path

    @property
    def image_url(self) -> str | None:
        """
        Return the URL of the card image.
        """
        return self.image_path
    
    @staticmethod
    def get_card_data(card_name: str) -> dict | None:
        """
        Fetch card data from the external API by card name.
        """
        cleaned_name = ImageDownloader.clean_card_name(card_name)
        response = requests.get(API_URL, params={'name': cleaned_name})
        if response.status_code == 200:
            data = response.json()
            if data['data']:
                return data['data'][0]
            
    @staticmethod
    def get_card_data_by_id(card_id: int) -> dict | None:
        """
        Fetch card data from the external API by YGOPro ID.
        """
        response = requests.get(API_URL, params={'id': card_id})
        if response.status_code == 200:
            data = response.json()
            if data.get('data'):
                return data['data'][0]
        return None

    @staticmethod
    def _download(card_name: str) -> str | None:
        """
        Download the image of a card by its name.
        Returns the path to the image.
        """
        data = ImageDownloader.get_card_data(card_name)
        if data:
            image_url = data['card_images'][0]['image_url']
            image_response = requests.get(image_url)
            if image_response.status_code == 200:
                # Save the image to the local folder
                image_path_in_media = IMAGE_FOLDER + card_name.replace(" ", "_").lower() + ".jpg"
                image_path = default_storage.save(image_path_in_media, ContentFile(image_response.content))
                return image_path
        
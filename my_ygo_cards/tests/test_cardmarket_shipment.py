from datetime import datetime
from decimal import Decimal
from django.test import TestCase

from my_ygo_cards.shipment_reader.cardmarket import CardmarketShipmentReader
from pypdf import PdfReader

SHIPMENT_PATHS = [
    "my_ygo_cards/tests/doc/Purchase_#1215740331.pdf",
    "my_ygo_cards/tests/doc/Purchase_#1217063385.pdf",
    "my_ygo_cards/tests/doc/Purchase_#1220691336.pdf",
    "my_ygo_cards/tests/doc/Purchase_#1211746704.pdf",
    "my_ygo_cards/tests/doc/Purchase_#1169478602.pdf",
    "my_ygo_cards/tests/doc/Purchase_#1231846416.pdf",
]


SALES_SHIPMENT_PATHS = [
    "my_ygo_cards/tests/doc/Sale_#1229655135.pdf",
    "my_ygo_cards/tests/doc/Sale_#1229772237.pdf",
]

class CardmarketShipmentGenericTests(TestCase):
    def setUp(self):
        self.shipment_data = [
            PdfReader(path) for path in SHIPMENT_PATHS
        ]

    def test_cardmarket_shipment(self):
        for shipment in self.shipment_data:
            with self.subTest(shipment=shipment):
                self.assertEqual(len(shipment.pages), 1, "Shipment should have exactly one page.")
                page = shipment.pages[0]
                text = page.extract_text() if page else ""
                self.assertIsInstance(text, str, "Extracted text should be a string.")
                self.assertGreater(len(text), 0, "Extracted text should not be empty.")


class CardmarketShipmentReader1Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[0]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes

    def test_shipment_reader_1(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "THUNDERKINGGAMES", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")
        self.assertEqual(data['price'], Decimal('18.17'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("12.06.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")

        received_date_ref = datetime.strptime("19.06.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 33, "Cards data should not be empty.")

        # moonlight
        moonlight_cards = [card for card in cards_data if card['name'] == "Black Rose Moonlight Dragon (V.2 - Ultra Rare)"]
        self.assertEqual(len(moonlight_cards), 1, "There should be exactly one Black Rose Moonlight Dragon card.")
        self.assertEqual(moonlight_cards[0]['code'], "RA03-EN038", "Card code should match expected value.")
        self.assertEqual(moonlight_cards[0]['status'], "NM", "Card status should be NM.")
        self.assertEqual(moonlight_cards[0]['price'], Decimal('0.19'), "Card price should match expected value.")

        # sage with eyes of blue
        sage_cards = [card for card in cards_data if card['name'] == "Sage with Eyes of Blue"]
        self.assertEqual(len(sage_cards), 3, "There should be exactly 3 Sage with Eyes of Blue card.")

        for card in sage_cards:
            self.assertEqual(card['code'], "SDWD-EN013", "Card code should match expected value.")
            self.assertEqual(card['status'], "NM", "Card status should be NM.")
            self.assertEqual(card['price'], Decimal('0.08'), "Card price should match expected value.")

class CardmarketShipmentReader2Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[1]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes, can be removed later

    def test_shipment_reader_2(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "Arcana15", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")

        self.assertEqual(data['price'], Decimal('1.84'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("20.06.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")

        received_date_ref = datetime.strptime("01.07.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 3, "Cards data should not be empty.")

        # pre-preparation
        pre_preparation_cards = [card for card in cards_data if card['name'] == "Pre-Preparation of Rites (V.1 - Super Rare)"]
        self.assertEqual(len(pre_preparation_cards), 3, "There should be exactly 3 Pre-Preparation of Rites card.")
        for card in pre_preparation_cards:
            self.assertEqual(card['code'], "RA01-EN055", "Card code should match expected value.")
            self.assertEqual(card['status'], "EX", "Card status should be EX.")
            self.assertEqual(card['price'], Decimal('0.05'), "Card price should match expected value.")

class CardmarketShipmentReader3Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[2]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes, can be removed later

    def test_shipment_reader_3(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "fabiospinazzola", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")

        self.assertEqual(data['price'], Decimal('14.19'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("14.07.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")
        received_date_ref = datetime.strptime("22.07.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")


        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")

        self.assertEqual(len(cards_data), 1, "Cards data should not be empty.")

        mitsu_ritual_cards = [card for card in cards_data if card['name'] == "Mitsurugi Ritual (V.1 - Ultra Rare)"]
        self.assertEqual(len(mitsu_ritual_cards), 1, "There should be exactly one Mitsurugi Ritual card.")
        self.assertEqual(mitsu_ritual_cards[0]['code'], "SUDA-EN095", "Card code should match expected value.")
        self.assertEqual(mitsu_ritual_cards[0]['status'], "NM", "Card status should be NM.")




class CardmarketShipmentReader4Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[3]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes
    
    def test_shipment_reader_4(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "FireDemon", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")

        self.assertEqual(data['price'], Decimal('1.74'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("16.05.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")
        received_date_ref = datetime.strptime("24.05.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 1, "Cards data should not be empty.")

        # shogun
        shogun_cards = [card for card in cards_data if card['name'] == "Battle Shogun of the Six Samurai"]
        self.assertEqual(len(shogun_cards), 1, "There should be exactly one Battle Shogun of the Six Samurai card.")
        self.assertEqual(shogun_cards[0]['code'], "MGED-FR143", "Card code should match expected value.")
        self.assertEqual(shogun_cards[0]['status'], "EX", "Card status should be EX.")
    
class CardmarketShipmentReader5Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[4]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes

    def test_shipment_reader_5(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "dan960", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")

        self.assertEqual(data['price'], Decimal('39.60'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("12.08.2024", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")
        received_date_ref = datetime.strptime("22.08.2024", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 0, "Cards data should not be empty.")

class CardmarketShipmentReader6Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SHIPMENT_PATHS[5]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes, can be removed later
    
    def test_shipment_reader_6(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['seller_name'], "unrealarxinn", "Seller name should match expected value.")
        self.assertIsNone(data['buyer_name'], "Buyer name should be None (me).")

        self.assertEqual(data['price'], Decimal('11.08'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("19.09.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")
        received_date_ref = datetime.strptime("01.10.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 1, "Cards data should not be empty.")

        # zeus
        zeus_cards = [card for card in cards_data if card['name'] == "Divine Arsenal AA-ZEUS - Sky Thunder"]
        self.assertEqual(len(zeus_cards), 1, "There should be exactly one Divine Arsenal AA-ZEUS - Sky Thunder card.")
        self.assertEqual(zeus_cards[0]['code'], "STAS-EN044", "Card code should match expected value.")
        self.assertEqual(zeus_cards[0]['status'], "NM", "Card status should be NM.")

class CardmarketSalesShipmentReader1Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SALES_SHIPMENT_PATHS[0]).pages[0].extract_text()
        #print(self.shipment_data_str)  # For debugging purposes, can be removed later
    
    def test_sales_shipment_reader_1(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['buyer_name'], "CrowX974", "Buyer name should match expected value.")
        self.assertIsNone(data['seller_name'], "Seller name should be None (me).")

        self.assertEqual(data['price'], Decimal('8.69'), "Price should match expected value.")

        buy_date_ref = datetime.strptime("06.09.2025", "%d.%m.%Y").date()
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")

        received_date_ref = datetime.strptime("11.09.2025", "%d.%m.%Y").date()
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 2, "Cards data should not be empty.")
        # Magical Musketeer cross domination

        musketeer_cards = [card for card in cards_data if card['name'] == "Magical Musket - Cross-Domination"]
        self.assertEqual(len(musketeer_cards), 2, "There should be exactly 2 Magical Musket - Cross-Domination card.")

class CardmarketSalesShipmentReader2Tests(TestCase):
    def setUp(self):
        self.shipment_data_str = PdfReader(SALES_SHIPMENT_PATHS[1]).pages[0].extract_text()
        print(self.shipment_data_str)  # For debugging purposes, can be removed later
    
    def test_sales_shipment_reader_1(self):
        data = CardmarketShipmentReader.extract_dates_and_prices(self.shipment_data_str)
        self.assertIsInstance(data, dict, "Extracted data should be a dictionary.")
        self.assertEqual(data['buyer_name'], "Sinistrale", "Buyer name should match expected value.")
        self.assertIsNone(data['seller_name'], "Seller name should be None (me).")

        self.assertEqual(data['price'], Decimal('12.68'), "Price should match expected value.")

        buy_date_ref = None
        self.assertEqual(data['buy_date'], buy_date_ref, "Buy date should match expected value.")

        received_date_ref = None
        self.assertEqual(data['received_date'], received_date_ref, "Received date should match expected value.")

        cards_data = CardmarketShipmentReader.extract_cards(self.shipment_data_str)
        self.assertIsInstance(cards_data, list, "Extracted cards data should be a list.")
        self.assertEqual(len(cards_data), 6, "Cards data should not be empty.")
        
        # Magical Musketeer Caspar

        musketeer_1_cards = [card for card in cards_data if card['name'] == "Magical Musketeer Caspar"]
        self.assertEqual(len(musketeer_1_cards), 3, "There should be exactly 3 Magical Musketeer Caspar card.")
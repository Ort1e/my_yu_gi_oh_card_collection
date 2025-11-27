
from datetime import datetime
from decimal import Decimal
import re

ME = "0rtie"



class CardmarketShipmentReader:
    @staticmethod
    def extract_dates_and_prices(text : str):
        def extract(pattern) -> str:
            match = re.search(pattern, text)
            return match.group(1) if match else ""

        seller = extract(r'Seller\s*-\s*(.+)').strip()
        if seller == ME:
            seller = None
        
        buyer = extract(r'Buyer\s*-\s*(.+)').strip()
        if buyer == ME:
            buyer = None

        def parse_date(date_str: str):
            if not date_str:
                return None
            try:
                return datetime.strptime(date_str, "%d.%m.%Y").date()
            except ValueError:
                return None
            
        buy_date = parse_date(extract(r'Paid:\s*(\d{2}\.\d{2}\.\d{4})'))
        received_date = parse_date(extract(r'Arrived\s+(\d{2}\.\d{2}\.\d{4})'))

        return {
            'buy_date': buy_date,
            'received_date': received_date,
            'price': Decimal(extract(r'Total\s+([\d,]+)\s+EUR').replace(',', '.')),
            'seller_name': seller,
            'buyer_name': buyer,
        }
    
    @staticmethod
    def extract_cards(text : str):
        split = text.split("Yugioh Singles:")
        if len(split) < 2:
            return []

        card_text = split[1].strip()
        lines = card_text.splitlines()

        card_regex = re.compile(
            r'(?P<qty>\d+)\s+(?P<name>.+?)\s+X?(?P<code>(?:\d|[a-zA-Z]){3})\s+(?P<language_code>(?:EN)|(?:FR))\s+(?P<status>[A-Z]{2})\s+(?P<set>[A-Z0-9]+)\s+?.+\s+(?P<price>[\d,]+)\s+EUR'
        )

        cards = []
        for line in lines:
            match = card_regex.match(line)
            if match:
                data = match.groupdict()
                qty = int(data['qty'])
                full_code = f"{data['set']}-{data['language_code']}{data['code']}"
                for _ in range(qty):
                    cards.append({
                        'name': data['name'].strip(),
                        'code': full_code,
                        'status': data['status'],
                        'price': Decimal(data['price'].replace(',', '.')),
                    })

        
        return cards

from dataclasses import dataclass
import json
from typing import Any
from django.core.management.base import BaseCommand
from django.db import transaction
import requests
import datetime

from my_ygo_cards.models import AdvancedBanList, CardData  # adjust import as needed

from dataclasses import dataclass, field
from typing import Any, List, Optional
import json

import html
import re


def html_unescape(text: str) -> str:
    return html.unescape(text)

def format_strings(text: str) -> str:
    text = html_unescape(text)

    # replace the "<[...]>" by ""
    text = re.sub(r"<[^>]+>", "", text)
    # replace multiple spaces by a single space
    text = re.sub(r"\s+", " ", text)
    return text.strip()


@dataclass
class BanListJsonEntry:
    # Required
    nameeng: str

    # Optional fields
    frame: Optional[str] = None
    icon: Optional[str] = None
    namefra: Optional[str] = None
    nameger: Optional[str] = None
    nameita: Optional[str] = None
    namespa: Optional[str] = None
    link: Optional[str] = None
    cid: Optional[str] = None
    cardtypeeng: Optional[str] = None
    cardtypefra: Optional[str] = None
    cardtypeger: Optional[str] = None
    cardtypeita: Optional[str] = None
    cardtypespa: Optional[str] = None
    prev: Optional[int] = None

    @classmethod
    def from_json(cls, data: Any) -> "BanListJsonEntry":
        """
        Accepts either a JSON string or a Python dict.
        All fields are optional except nameeng.
        """
        if isinstance(data, str):
            data = json.loads(data)

        if "nameeng" not in data:
            raise ValueError("Missing required field: 'nameeng'")

        return cls(**data)


@dataclass
class BanListJsonData:
    _from: str
    _0: List[BanListJsonEntry]
    _1: List[BanListJsonEntry]
    _2: List[BanListJsonEntry]

    to: Optional[str] = None

    @classmethod
    def from_json(cls, data: Any) -> "BanListJsonData":
        if isinstance(data, str):
            data = json.loads(data)

        return cls(
            _from=data["from"],
            _0=[BanListJsonEntry.from_json(item) for item in data.get("0", data.get("_0", []))],
            _1=[BanListJsonEntry.from_json(item) for item in data.get("1", data.get("_1", []))],
            _2=[BanListJsonEntry.from_json(item) for item in data.get("2", data.get("_2", []))],
            to=data.get("to")
        )

        
class Command(BaseCommand):
    help = "Scrape the Yugioh Advanced Forbidden & Limited list and update the database"

    BAN_URL = "https://img.yugioh-card.com/eu/_data/fllists/current.json"

    def handle(self, *args, **options):
        self.stdout.write(f"Fetching ban list from {self.BAN_URL} …")

        response: requests.Response = requests.get(self.BAN_URL)
        response.raise_for_status()

        ban_data = BanListJsonData.from_json(response.text)

        # ------------------------------------------------------------------
        # 1. Parse the effective date
        # Example format from JSON: "2024-10-07"
        # ------------------------------------------------------------------
        try:
            effective_date = datetime.datetime.strptime(ban_data._from, "%d/%m/%Y").date()
        except ValueError:
            self.stderr.write(f"Invalid date format in JSON: {ban_data._from}")
            return

        self.stdout.write(f"Ban list effective from: {effective_date}")

        # ------------------------------------------------------------------
        # 2. Create/get the AdvancedBanList object
        # ------------------------------------------------------------------
        ban_list, created = AdvancedBanList.objects.get_or_create(date=effective_date)

        if created:
            self.stdout.write(self.style.SUCCESS(f"Created new AdvancedBanList for {effective_date}"))
        else:
            self.stdout.write(f"Using existing ban list — wiping old entries")
            ban_list.entries.all().delete()  # type: ignore

        # Mapping JSON sections to ban statuses
        section_status_map = {
            "_0": "Banned",
            "_1": "Limited",
            "_2": "Semi-Limited",
        }

        # ------------------------------------------------------------------
        # 3. Process each list (Forbidden, Limited, Semi-Limited)
        # ------------------------------------------------------------------
        total_imported = 0

        with transaction.atomic():

            for section_name, status in section_status_map.items():
                section_entries: list[BanListJsonEntry] = getattr(ban_data, section_name)

                self.stdout.write(f"Processing {status}: {len(section_entries)} cards")

                for entry in section_entries:
                    name = format_strings(entry.nameeng)
                    self.stdout.write(f"  - {entry.nameeng} ({name})")
                    # Resolve CardData
                    # You can expand this if your CardData model uses other fields
                    card_data = CardData.get_or_fetch(
                        html_unescape(name)
                    )

                    # Create BanListEntry
                    ban_list.entries.create( # type: ignore
                        card_data=card_data,
                        status=status
                    )

                    total_imported += 1

        self.stdout.write(
            self.style.SUCCESS(
                f"Successfully imported {total_imported} cards for the {effective_date} ban list."
            )
        )

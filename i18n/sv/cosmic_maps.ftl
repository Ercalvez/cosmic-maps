app-title = Cosmic kartor
about = Om
repository = Källkod
view = Visa
zoom-in = Zooma in
zoom-out = Zooma ut
show-location = Visa din plats 
git-description = Git commit {$hash} på {$date}

search-placeholder = Sök efter platser

## Vägbeskrivning

directions = Vägbeskrivning
directions-from = Från
directions-to = Till
directions-choose-start = Välj start
directions-choose-destination = Välj destination
directions-swap = Växla start och destination
directions-back = Tillbaka till plats
your-location = Din plats
travel-mode-foot = Promenad
travel-mode-bike = Cykel
travel-mode-transit = Kollektivtrafik
travel-mode-car = Bil

## Rutter

route-loading = Hittar rutter…
route-not-found = Ingen rutt hittades.
route-failed = Kunde inte hitta rutter. Kontrollera din anslutning.
route-earlier = Tidigare
route-later = Senare
route-minutes = { $minutes } min
route-hours = { $hours } t { $minutes }
route-meters = { $meters } m
route-kilometers = { $kilometers } km
transit-metro = Tunnelbana
transit-bus = Buss
transit-tram = Spårvagn
transit-train = Tåg
transit-ferry = Färja
route-walk-only = Bara promenad
route-transfers = { $count ->
    [0] Inget byte
    [one] 1 byte
   *[other] { $count } Flera byten
}
route-leg-walk = Promenad { $distance } till { $place }
route-leg-towards = mot { $headsign }
route-leg-board = Gå ombord på { $stop }
route-leg-alight = Kliv av på { $stop }
route-leg-stops = { $count ->
    [0] Inga stopp däremellan
    [one] 1 stopp
   *[other] { $count } flera stopp
}
route-attribution = Rutter:

## Gångsteg, utifrån MOTIS relativa riktningar

step-depart = Start
step-continue = Fortsätt
step-left = Sväng vänster
step-right = Sväng höger
step-slightly-left = Håll åt vänster
step-slightly-right = Håll åt höger
step-hard-left = Sväng skarpt vänster
step-hard-right = Sväng skarpt höger
step-roundabout = Gå runt rondellen
step-uturn = Gör en U-sväng
step-elevator = Ta hissen
step-stairs = Ta trapporna
step-enter-station = Gå in på stationen
step-exit-station = Gå ut stationen
step-follow-signs = Följ skyltarna
step-onto = { $action } till { $street }

## POI detaljer

poi-details-loading = Laddar detaljer…
poi-looking-up = Tittar upp…
poi-wheelchair-yes = Rullstolsanpassat
poi-wheelchair-limited = Delvis rullstolsanpassat
poi-wheelchair-no = Inte rullstolsanpassat

## POI kategorier, identifierade utifrån värdet för `subclass` eller `class` i OpenMapTiles.
## Saknade värden visas som det råa värdet, gjort läsbart.

# Mat och dryck 
poi-type-restaurant = Restaurang
poi-type-cafe = Café
poi-type-fast_food = Snabbmat
poi-type-bar = Bar
poi-type-pub = Pub
poi-type-beer = Öl
poi-type-nightclub = Nattklubb
poi-type-ice_cream = Glass
poi-type-bakery = Bageri

# Butiker
poi-type-shop = Butiker
poi-type-clothes = Klädbutik
poi-type-clothing_store = Klädbutik
poi-type-shoes = Skoaffär
poi-type-convenience = Närbutik
poi-type-supermarket = Stormarknad
poi-type-grocery = Livsmedelsbutik
poi-type-alcohol_shop = Spritbutik
poi-type-beauty = Skönhetssalong
poi-type-hairdresser = Frisör
poi-type-cosmetics = Kosmetikbutik
poi-type-gift = Presentbutik
poi-type-jewelry = Juvelerare
poi-type-optician = Optiker
poi-type-florist = Florist
poi-type-books = Bokhandel
poi-type-music = Musikaffär
poi-type-bicycle = Cykelaffär
poi-type-car = Bilhandlare
poi-type-laundry = Tvätteri


# Kultur och fritid
poi-type-artwork = Konstverk
poi-type-art = Konstbutik
poi-type-gallery = Konstgalleri
poi-type-art_gallery = Konstgalleri
poi-type-museum = Museum
poi-type-attraction = Attraktion
poi-type-castle = Slott
poi-type-theatre = Teater
poi-type-cinema = Bio
poi-type-zoo = Zoo
poi-type-escape_game = Escape room
poi-type-park = Park
poi-type-garden = Trädgård
poi-type-playground = Lekplats
poi-type-sports_centre = Sportcenter
poi-type-pitch = Sportplan
poi-type-stadium = Stadium
poi-type-swimming = Simbassäng
poi-type-yoga = Yogastudio

# Transport
poi-type-bus = Busshållplats
poi-type-bus_stop = Busshållplats
poi-type-tram_stop = Spårvagnshållplats
poi-type-subway = Tunnelbanestation
poi-type-subway_entrance = Tunnelbaneingång
poi-type-railway = Tågstation
poi-type-station = Tågstation
poi-type-halt = Tågstation
poi-type-ferry_terminal = Färjeterminal
poi-type-bicycle_rental = Cykeluthyrning
poi-type-fuel = Bensinstation
poi-type-charging_station = Laddstation
poi-type-parking = Parkering
poi-type-entrance = Entré
poi-type-gate = Gate

# Logi
poi-type-lodging = Ackommodation
poi-type-hotel = Hotell
poi-type-hostel = Vandrarhem

# Tjänster
poi-type-atm = Bankomat
poi-type-bank = Bank
poi-type-post = Postkontor
poi-type-post_office = Postkontor
poi-type-parcel_locker = Paketskåp
poi-type-information = Information
poi-type-board = Informationstavla
poi-type-toilets = Toaletter
poi-type-office = Kontor
poi-type-company = Företag
poi-type-association = Förening
poi-type-it = IT Företag
poi-type-government = Statlig myndighet
poi-type-ngo = Icke-statlig organisation
poi-type-diplomatic = Ambassad
poi-type-hackerspace = Hackerspace

# Offentliga tjänster, utbildning och hälsa
poi-type-town_hall = Stadshus
poi-type-community_centre = Bygdegård
poi-type-police = Polis
poi-type-fire_station = Brandstation
poi-type-school = Skola
poi-type-kindergarten = Förskola
poi-type-college = Högskola
poi-type-university = Universitet
poi-type-library = Bibliotek
poi-type-hospital = Sjukhus
poi-type-clinic = Klinik
poi-type-doctors = Doktor
poi-type-dentist = Tandläkare
poi-type-pharmacy = Apotek
poi-type-veterinary = Veterinär

# Adresser och områden, som visas när man klickar på kartan
poi-type-house = Address
poi-type-building = Byggnad
poi-type-highway = Gata
poi-type-boundary = Område
poi-type-administrative = Administrativt område
poi-type-country = Land
poi-type-state = Region
poi-type-city = Stad
poi-type-town = Stad
poi-type-village = By
poi-type-suburb = Distrikt
poi-type-neighbourhood = Grannskap
poi-type-river = Flod
poi-type-canal = Kanal

# Gudstjänstlokaler, kategoriserade efter religion
poi-type-place_of_worship = Plats för religionsutövning
poi-type-christian = Kyrka
poi-type-muslim = Moské
poi-type-jewish = Synagoga
poi-type-cemetery = Kyrkogård

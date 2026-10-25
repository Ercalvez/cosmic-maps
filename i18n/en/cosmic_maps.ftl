app-title = Cosmic Maps
about = About
repository = Repository
view = View
zoom-in = Zoom in
zoom-out = Zoom out
show-location = Show your location
git-description = Git commit {$hash} on {$date}

search-placeholder = Search for places

## Directions

directions = Directions
directions-from = From
directions-to = To
directions-choose-start = Choose start
directions-choose-destination = Choose destination
directions-swap = Swap start and destination
directions-back = Back to place
your-location = Your location
travel-mode-foot = Walk
travel-mode-bike = Bike
travel-mode-transit = Transit
travel-mode-car = Car

## Routes

route-loading = Finding routes…
route-not-found = No route found.
route-failed = Could not find routes. Check your connection.
route-unavailable = Routes for this mode are not available yet.
route-earlier = Earlier
route-later = Later
route-minutes = { $minutes } min
route-hours = { $hours } h { $minutes }
route-meters = { $meters } m
route-kilometers = { $kilometers } km
transit-metro = Metro
transit-bus = Bus
transit-tram = Tram
transit-train = Train
transit-ferry = Ferry
route-walk-only = Walk only
route-transfers = { $count ->
    [0] No transfers
    [one] 1 transfer
   *[other] { $count } transfers
}
route-leg-walk = Walk { $distance } to { $place }
route-leg-towards = towards { $headsign }
route-leg-board = Board at { $stop } · { $time }
route-leg-alight = Get off at { $stop } · { $time }
route-leg-stops = { $count ->
    [0] No stops in between
    [one] 1 stop
   *[other] { $count } stops
}
route-attribution = Routes:

## Walking steps, from MOTIS' relative directions

step-depart = Start
step-continue = Continue
step-left = Turn left
step-right = Turn right
step-slightly-left = Bear left
step-slightly-right = Bear right
step-hard-left = Turn sharp left
step-hard-right = Turn sharp right
step-roundabout = Go around the roundabout
step-uturn = Make a U-turn
step-elevator = Take the elevator
step-stairs = Take the stairs
step-enter-station = Enter the station
step-exit-station = Exit the station
step-follow-signs = Follow the signs
step-onto = { $action } onto { $street }

## POI details

poi-details-loading = Loading details…
poi-looking-up = Looking up…
poi-wheelchair-yes = Wheelchair accessible
poi-wheelchair-limited = Partially wheelchair accessible
poi-wheelchair-no = Not wheelchair accessible

## POI categories, keyed by OpenMapTiles `subclass` or `class` value.
## Missing values are shown as the raw value, made readable.

# Food and drink
poi-type-restaurant = Restaurant
poi-type-cafe = Café
poi-type-fast_food = Fast food
poi-type-bar = Bar
poi-type-pub = Pub
poi-type-beer = Pub
poi-type-nightclub = Nightclub
poi-type-ice_cream = Ice cream
poi-type-bakery = Bakery

# Shops
poi-type-shop = Shop
poi-type-clothes = Clothing store
poi-type-clothing_store = Clothing store
poi-type-shoes = Shoe store
poi-type-convenience = Convenience store
poi-type-supermarket = Supermarket
poi-type-grocery = Grocery store
poi-type-alcohol_shop = Liquor store
poi-type-beauty = Beauty salon
poi-type-hairdresser = Hairdresser
poi-type-cosmetics = Cosmetics store
poi-type-gift = Gift shop
poi-type-jewelry = Jewelry store
poi-type-optician = Optician
poi-type-florist = Florist
poi-type-books = Bookstore
poi-type-music = Music store
poi-type-bicycle = Bicycle shop
poi-type-car = Car dealer
poi-type-laundry = Laundry

# Culture and leisure
poi-type-artwork = Artwork
poi-type-art = Art shop
poi-type-gallery = Art gallery
poi-type-art_gallery = Art gallery
poi-type-museum = Museum
poi-type-attraction = Attraction
poi-type-castle = Castle
poi-type-theatre = Theater
poi-type-cinema = Cinema
poi-type-zoo = Zoo
poi-type-escape_game = Escape room
poi-type-park = Park
poi-type-garden = Garden
poi-type-playground = Playground
poi-type-sports_centre = Sports center
poi-type-pitch = Sports field
poi-type-stadium = Stadium
poi-type-swimming = Swimming pool
poi-type-yoga = Yoga studio

# Transport
poi-type-bus = Bus stop
poi-type-bus_stop = Bus stop
poi-type-tram_stop = Tram stop
poi-type-subway = Subway station
poi-type-subway_entrance = Subway entrance
poi-type-railway = Train station
poi-type-station = Train station
poi-type-halt = Train station
poi-type-ferry_terminal = Ferry terminal
poi-type-bicycle_rental = Bicycle rental
poi-type-fuel = Gas station
poi-type-charging_station = Charging station
poi-type-parking = Parking
poi-type-entrance = Entrance
poi-type-gate = Gate

# Lodging
poi-type-lodging = Accommodation
poi-type-hotel = Hotel
poi-type-hostel = Hostel

# Services
poi-type-atm = ATM
poi-type-bank = Bank
poi-type-post = Post office
poi-type-post_office = Post office
poi-type-parcel_locker = Parcel locker
poi-type-information = Information
poi-type-board = Information board
poi-type-toilets = Toilets
poi-type-office = Office
poi-type-company = Company
poi-type-association = Association
poi-type-it = IT company
poi-type-government = Government office
poi-type-ngo = NGO
poi-type-diplomatic = Embassy
poi-type-hackerspace = Hackerspace

# Public services, education and health
poi-type-town_hall = Town hall
poi-type-community_centre = Community center
poi-type-police = Police
poi-type-fire_station = Fire station
poi-type-school = School
poi-type-kindergarten = Kindergarten
poi-type-college = College
poi-type-university = University
poi-type-library = Library
poi-type-hospital = Hospital
poi-type-clinic = Clinic
poi-type-doctors = Doctor
poi-type-dentist = Dentist
poi-type-pharmacy = Pharmacy
poi-type-veterinary = Veterinarian

# Addresses and areas, as found by clicking the map
poi-type-house = Address
poi-type-building = Building
poi-type-highway = Street
poi-type-boundary = Area
poi-type-administrative = Administrative area
poi-type-country = Country
poi-type-state = Region
poi-type-city = City
poi-type-town = Town
poi-type-village = Village
poi-type-suburb = District
poi-type-neighbourhood = Neighborhood
poi-type-river = River
poi-type-canal = Canal

# Places of worship, keyed by religion
poi-type-place_of_worship = Place of worship
poi-type-christian = Church
poi-type-muslim = Mosque
poi-type-jewish = Synagogue
poi-type-cemetery = Cemetery

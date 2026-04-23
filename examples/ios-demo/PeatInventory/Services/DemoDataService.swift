import Foundation

enum DemoDataService {
    static let sampleItems: [InventoryItem] = [
        // Communications
        InventoryItem(nsn: "5820-01-451-8250", nomenclature: "RADIO SET, AN/PRC-152A", serialNumber: "W925692", quantity: 4, conditionCode: .A, location: "CP ALPHA, MR 38S MB 4312 0678", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .communications, notes: "COMSEC loaded, due for PMCS 15MAR"),
        InventoryItem(nsn: "5820-01-505-1523", nomenclature: "RADIO SET, AN/PRC-117G", serialNumber: "W834110", quantity: 2, conditionCode: .A, location: "CP ALPHA, MR 38S MB 4312 0678", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SPC Nguyen", category: .communications, notes: "SATCOM capable, antenna kit complete"),
        InventoryItem(nsn: "5820-01-567-4830", nomenclature: "ANTENNA GROUP, OE-254", serialNumber: "A2249871", quantity: 1, conditionCode: .B, location: "CP ALPHA, MR 38S MB 4312 0678", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "CPT Rivera", category: .communications, notes: "Guy wire needs replacement"),
        InventoryItem(nsn: "5810-01-574-6321", nomenclature: "HARRIS FALCON III MANPACK", serialNumber: "H7734921", quantity: 2, conditionCode: .A, location: "OP NORTH, MR 38S MB 4298 0701", responsibleUnit: "2nd PLT, A CO, 2-506 IN", responsiblePerson: "SSG Kim", category: .communications),

        // Optics
        InventoryItem(nsn: "5855-01-432-0524", nomenclature: "NIGHT VISION DEVICE, AN/PVS-14", serialNumber: "NV20451", quantity: 12, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .optics, notes: "Full set, tubes above avg"),
        InventoryItem(nsn: "5855-01-647-6498", nomenclature: "NIGHT VISION GOGGLE, ENVG-B", serialNumber: "EV30287", quantity: 4, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .optics, notes: "Rapid target acquisition mode enabled"),
        InventoryItem(nsn: "5855-01-540-2650", nomenclature: "THERMAL SIGHT, AN/PAS-13G(V)1", serialNumber: "TS44891", quantity: 4, conditionCode: .C, location: "MAINTENANCE, BLDG 4150", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .optics, notes: "Awaiting depot-level repair, focal array degraded"),
        InventoryItem(nsn: "1240-01-411-1265", nomenclature: "BINOCULAR, M22", serialNumber: "BN88102", quantity: 6, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "1LT Chen", category: .optics),

        // Weapons
        InventoryItem(nsn: "1005-01-382-0953", nomenclature: "RIFLE, 5.56MM, M4A1", serialNumber: "W485921", quantity: 30, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .weapons, notes: "Full PLT complement, RFI complete"),
        InventoryItem(nsn: "1005-01-567-1234", nomenclature: "MACHINE GUN, 7.62MM, M240B", serialNumber: "M24-8834", quantity: 4, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SPC Martinez", category: .weapons),
        InventoryItem(nsn: "1005-01-484-0327", nomenclature: "MACHINE GUN, 5.56MM, M249 SAW", serialNumber: "SAW-6621", quantity: 6, conditionCode: .B, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "PFC Johnson", category: .weapons, notes: "Feed tray cover spring weak on 2 units"),
        InventoryItem(nsn: "1010-01-540-3960", nomenclature: "LAUNCHER, GRENADE, M320", serialNumber: "GL-9021", quantity: 4, conditionCode: .A, location: "ARMS ROOM, BLDG 4120", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .weapons),

        // Vehicles
        InventoryItem(nsn: "2320-01-518-3447", nomenclature: "TRUCK, UTILITY, HMMWV M1151", serialNumber: "HQ-2241", quantity: 4, conditionCode: .A, location: "MOTOR POOL, BLDG 4200", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SPC Davis", category: .vehicles, notes: "PMCS current, mileage 42,100"),
        InventoryItem(nsn: "2320-01-380-8937", nomenclature: "TRUCK, CARGO, LMTV M1078A1", serialNumber: "LM-1193", quantity: 2, conditionCode: .D, location: "MAINTENANCE, BLDG 4210", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "SGT Williams", category: .vehicles, notes: "Transmission replacement scheduled, ECD 20MAR"),
        InventoryItem(nsn: "2350-01-565-0932", nomenclature: "MINE RESISTANT VEHICLE, M-ATV", serialNumber: "MA-0487", quantity: 2, conditionCode: .A, location: "MOTOR POOL, BLDG 4200", responsibleUnit: "2nd PLT, A CO, 2-506 IN", responsiblePerson: "SSG Kim", category: .vehicles),

        // Medical
        InventoryItem(nsn: "6545-01-530-0929", nomenclature: "FIRST AID KIT, INDIVIDUAL (IFAK)", serialNumber: "IFAK-LOT-2024A", quantity: 40, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "68W SPC Medina", category: .medical, notes: "All components within expiration"),
        InventoryItem(nsn: "6530-01-492-2180", nomenclature: "LITTER, FOLDING, POLELESS (SKED)", serialNumber: "SK-0044", quantity: 8, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "68W SPC Medina", category: .medical),
        InventoryItem(nsn: "6515-01-560-7600", nomenclature: "DEFIBRILLATOR, AUTOMATIC EXTERNAL", serialNumber: "AED-3391", quantity: 2, conditionCode: .A, location: "AID STATION, BLDG 4100", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "68W SPC Medina", category: .medical, notes: "Pads expire JUN2025, batteries good"),
        InventoryItem(nsn: "6545-01-587-2210", nomenclature: "COMBAT MEDIC SET, M17", serialNumber: "CMS-7789", quantity: 2, conditionCode: .A, location: "AID STATION, BLDG 4100", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "68W SPC Medina", category: .medical),

        // Power & Electrical
        InventoryItem(nsn: "6115-01-274-7387", nomenclature: "GENERATOR SET, 5KW MEP-802A", serialNumber: "GEN-4492", quantity: 2, conditionCode: .A, location: "CP ALPHA, MR 38S MB 4312 0678", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "SPC Brown", category: .powerAndElectrical, notes: "650 hrs runtime, oil change due at 700"),
        InventoryItem(nsn: "6115-01-575-4060", nomenclature: "GENERATOR SET, 10KW MEP-813A", serialNumber: "GEN-8810", quantity: 1, conditionCode: .F, location: "MAINTENANCE, BLDG 4150", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "SPC Brown", category: .powerAndElectrical, notes: "Voltage regulator failed, parts on order"),
        InventoryItem(nsn: "6140-01-490-4316", nomenclature: "BATTERY, LITHIUM, BB-2590/U", serialNumber: "BAT-LOT-2024B", quantity: 48, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SPC Nguyen", category: .powerAndElectrical),
        InventoryItem(nsn: "6145-01-504-1210", nomenclature: "CABLE ASSEMBLY, POWER, 100FT", serialNumber: "CBL-2298", quantity: 6, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "A CO HQ, 2-506 IN", responsiblePerson: "SPC Brown", category: .powerAndElectrical),

        // Other
        InventoryItem(nsn: "8465-01-524-7309", nomenclature: "RUCKSACK, MOLLE, LARGE", serialNumber: "RK-LOT-2023C", quantity: 36, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SGT Torres", category: .other),
        InventoryItem(nsn: "5985-01-517-4257", nomenclature: "ANTENNA, WHIP, AT-271A/PRC", serialNumber: "ANT-LOT-2024A", quantity: 20, conditionCode: .A, location: "SUPPLY ROOM, BLDG 4130", responsibleUnit: "1st PLT, A CO, 2-506 IN", responsiblePerson: "SPC Nguyen", category: .communications),
    ]

    static let demoNodeNames = [
        "SGT Torres - 1st PLT",
        "CPT Rivera - A CO HQ",
        "SSG Kim - 2nd PLT",
        "1LT Chen - A CO XO",
        "SFC Daniels - A CO PSG",
    ]
}

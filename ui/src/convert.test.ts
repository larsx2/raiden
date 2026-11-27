import { describe, expect, it } from "vitest";
import { convert } from "./convert";

const RATES = { USD: 1, EUR: 0.9, MXN: 18, GBP: 0.8 };

describe("convert", () => {
  it("converts between units of the same kind", () => {
    expect(convert("10 kg to lb")?.to).toBe("22.04622622 lb");
    expect(convert("1 mi in km")?.to).toBe("1.609344 km");
    expect(convert("5 in in cm")?.to).toBe("12.7 cm");
    expect(convert("1,000 ft to m")?.to).toBe("304.8 m");
    expect(convert("1,5 kg to g")?.to).toBe("1500 g");
    expect(convert("100 km/h to mph")?.to).toBe("62.13711922 mph");
    expect(convert("1 atm to psi")?.to).toBe("14.69594878 psi");
    expect(convert("2 acres to m2")?.to).toBe("8093.712845 m²");
    expect(convert("1 GiB to MB")?.to).toBe("1073.741824 MB");
    expect(convert("3 nautical miles to km")?.to).toBe("5.556 km");
  });

  it("handles the offsets of temperatures", () => {
    expect(convert("100 c to f")?.to).toBe("212 °F");
    expect(convert("-40°F to °C")?.to).toBe("-40 °C");
    expect(convert("0 K to celsius")?.to).toBe("-273.15 °C");
  });

  it("converts currencies with the rates it is given", () => {
    expect(convert("100 usd to mxn", RATES)).toEqual({
      from: "100.00 USD",
      to: "1,800.00 MXN",
      value: "1800.00",
    });
    expect(convert("$90 to eur", RATES)?.to).toBe("81.00 EUR");
    expect(convert("10 euros in gbp", RATES)?.to).toBe("8.89 GBP");
    expect(convert("100 usd to mxn")).toBeUndefined();
  });

  it("stays quiet for anything it cannot convert", () => {
    for (const text of [
      "",
      "kg to lb",
      "10 kg",
      "10 kg to km",
      "10 kg to kg",
      "10 foo to bar",
      "100 to eur",
      "how to cook rice",
    ]) {
      expect(convert(text, RATES)).toBeUndefined();
    }
  });
});

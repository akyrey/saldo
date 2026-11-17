import { describe, expect, it } from "vitest";
import { acceptRequest, frequencyLabel, suggestionForm } from "./suggestion-model";

const CANDIDATE = {
  account_id: "gym",
  account_name: "Gym",
  currency: "EUR",
  frequency: "half_year",
  amount_min: "29.00",
  amount_max: "30.00",
  first_payment: "2025-11-03",
  last_payment: "2026-11-03",
  next_due: "2027-05-03",
  split_ids: ["s1", "s2", "s3"],
  confidence: 100,
};

describe("suggestions", () => {
  it("labels frequencies", () => {
    expect(frequencyLabel("half_year")).toBe("Every six months");
    expect(frequencyLabel("monthly")).toBe("Monthly");
    expect(frequencyLabel("fortnightly")).toBe("fortnightly");
  });

  it("prefills the form from the candidate and builds the request", () => {
    const form = suggestionForm(CANDIDATE);
    expect(form).toEqual({ name: "Gym", amountMin: "29.00", amountMax: "30.00" });
    expect(acceptRequest(CANDIDATE, { ...form, name: " Gym club ", amountMax: "31,5" })).toEqual({
      account_id: "gym",
      currency: "EUR",
      name: "Gym club",
      frequency: "half_year",
      amount_min: "29.00",
      amount_max: "31.5",
      split_ids: ["s1", "s2", "s3"],
    });
  });
});

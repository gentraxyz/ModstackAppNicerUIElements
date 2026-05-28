export type SkinModel = "slim" | "classic";

export async function getMinecraftProfile(username: string) {
  const res = await fetch(
    `https://sessionserver.mojang.com/session/minecraft/profile/${username}`
  );

  if (!res.ok) throw new Error("Profile fetch failed");

  return res.json();
}

export function getSkinModelFromProfile(profile: any): SkinModel {
  try {
    const value = profile?.properties?.[0]?.value;
    if (!value) return "classic";

    const decoded = JSON.parse(atob(value));

    const model =
      decoded?.textures?.SKIN?.metadata?.model;

    return model === "slim" ? "slim" : "classic";
  } catch {
    return "classic";
  }
}

export function getSkinUrl(username: string) {
  return `https://mineskin.eu/skin/${username}`;
}
import { useState } from "react";
import { useAuth } from "../stores/authContext";
import {
  Dropdown,
  Modal,
  Button,
  TextField,
  Label,
  Input,
  useOverlayState,
  Tooltip,
} from "@heroui/react";
import { IconLogin, IconLogout, IconShoppingCart } from "@tabler/icons-react";
import Ms from "./icons/Ms";
import { open } from "@tauri-apps/plugin-shell";

export default function UserBtn() {
  const {
    authReady,
    user,
    loginWithMicrosoft,
    loginWithMojang,
    isWaiting,
    logout,
  } = useAuth();

  const modalState = useOverlayState();
  const [offlineUsername, setOfflineUsername] = useState("");

  const skinHelmURL = (name: string) =>
    `https://mineskin.eu/helm/${name}/40.png`;

  const getUserType = (user: User) => {
    switch (user.type) {
      case "microsoft":
        return "Microsoft";
      case "offline":
        return "Offline";
      default:
        return "";
    }
  };

  const handleLoginMicrosoft = async () => {
    modalState.open();
    try {
      await loginWithMicrosoft();
    } catch (e) {
      console.error("Login Microsoft failed", e);
    } finally {
      modalState.close();
    }
  };

  const handleOpenOfflineModal = () => {
    modalState.open();
  };

  const handleOfflineSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    loginWithMojang(offlineUsername)
      .then(() => {
        modalState.close();
        setOfflineUsername("");
      })
      .catch(console.error);
  };

  const handleOfflineCancel = () => {
    modalState.close();
    setOfflineUsername("");
  };

  const handleLogout = () => {
    modalState.close();
    logout();
  };

  if (!authReady) return null;

  return (
    <>
      <Dropdown>
        <Tooltip delay={0}>
          <Button variant="tertiary" size="lg" isIconOnly className="p-1">
            <img
              src={
                user?.minecraft?.name
                  ? skinHelmURL(user.minecraft.name)
                  : "./steve-helm.png"
              }
              alt={user?.minecraft?.name || "Steve"}
              className="size-full rounded"
            />
          </Button>
          <Tooltip.Content placement="right" offset={8} className="text-sm font-semibold">
            <p>{user?.minecraft?.name || "Not logged in"}</p>
          </Tooltip.Content>
        </Tooltip>

        <Dropdown.Popover className="min-w-60">
          <Dropdown.Menu>
            {!user ? (
              <>
                <Dropdown.Item onPress={handleLoginMicrosoft}>
                  <div className="flex items-center gap-2">
                    <Ms className="w-4 h-4" />
                    <span>Sign in with Microsoft</span>
                  </div>
                </Dropdown.Item>

                <Dropdown.Item onPress={handleOpenOfflineModal}>
                  <div className="flex items-center gap-2">
                    <IconLogin className="w-4 h-4" />
                    <span>Play offline</span>
                  </div>
                </Dropdown.Item>
              </>
            ) : (
              <>
                <Dropdown.Item className="cursor-default">
                  <div className="flex flex-col gap-1 pointer-events-none">
                    <p className="font-semibold">
                      {user?.minecraft?.name}
                    </p>
                    <p className="text-xs text-muted">
                      {getUserType(user)}
                    </p>
                  </div>
                </Dropdown.Item>

                <Dropdown.Item
                  variant="danger"
                  className="data-[hover=true]:bg-danger/40"
                  onPress={handleLogout}
                >
                  <div className="flex items-center gap-2">
                    <IconLogout className="w-4 h-4" />
                    <span>Sign out</span>
                  </div>
                </Dropdown.Item>
              </>
            )}
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>

      <Modal.Backdrop
        isDismissable={false}
        isKeyboardDismissDisabled={isWaiting}
        isOpen={modalState.isOpen}
        onOpenChange={modalState.setOpen}
      >
        <Modal.Container>
          <Modal.Dialog>
            <Modal.Header>
              <Modal.Heading>
                {isWaiting ? "Signing in..." : "Play offline"}
              </Modal.Heading>
            </Modal.Header>

            <Modal.Body className="p-2 gap-4">
              {!isWaiting ? (
                <form
                  onSubmit={handleOfflineSubmit}
                  onReset={handleOfflineCancel}
                  className="flex flex-col gap-4"
                >
                  <TextField
                    variant="secondary"
                    name="username"
                    isRequired
                    value={offlineUsername}
                    onChange={(val) => setOfflineUsername(val.replace(/[^a-zA-Z0-9_]/g, ""))}
                    autoFocus
                  >
                    <Label>Username</Label>
                    <Input
                      placeholder="Between 3 and 16 characters"
                      minLength={3}
                      maxLength={16}
                    />
                  </TextField>

                  <div className="flex gap-2 justify-end">
                    <Button
                      type="reset"
                      variant="secondary"
                      size="sm"
                      fullWidth
                    >
                      Cancel
                    </Button>

                    <Button
                      type="submit"
                      size="sm"
                      fullWidth
                      isDisabled={
                        offlineUsername.length < 3 ||
                        offlineUsername.length > 16
                      }
                    >
                      Confirm
                    </Button>
                  </div>

                  <hr className="border-border/40" />

                  <Button
                    variant="secondary"
                    size="sm"
                    fullWidth
                    onPress={() => open("https://www.minecraft.net/store")}
                    className="text-blue-400 border-blue-900 bg-blue-950/40 hover:bg-blue-900/40"
                  >
                    <IconShoppingCart className="w-4 h-4" />
                    Buy Minecraft
                  </Button>
                </form>
              ) : (
                <p className="text-center text-sm text-muted">
                  Please wait...
                </p>
              )}
            </Modal.Body>
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </>
  );
}
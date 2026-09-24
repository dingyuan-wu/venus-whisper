// naive-icons 默认描边是近黑色，这里统一改成继承文字色。
import type { ComponentType, SVGProps } from "react";
import * as N from "naive-icons";

type IconProps = SVGProps<SVGSVGElement> & { size?: number | string; color?: string; strokeWidth?: number | string };

const wrap = (C: ComponentType<IconProps>) => (p: IconProps) => <C size={16} color="currentColor" {...p} />;

export const ArrowLeft = wrap(N.ArrowLeftIcon);
export const ArrowUp = wrap(N.ArrowUpIcon);
export const Chat = wrap(N.ChatIcon);
export const Check = wrap(N.CheckIcon);
export const ChevronDown = wrap(N.ChevronDownIcon);
export const ChevronLeft = wrap(N.ChevronLeftIcon);
export const ChevronRight = wrap(N.ChevronRightIcon);
export const Close = wrap(N.CloseIcon);
export const Copy = wrap(N.CopyIcon);
export const File = wrap(N.FileIcon);
export const Flower = wrap(N.FlowerIcon);
export const Folder = wrap(N.FolderIcon);
export const Globe = wrap(N.GlobeIcon);
export const Key = wrap(N.KeyIcon);
export const Menu = wrap(N.MenuIcon);
export const Pencil = wrap(N.PencilIcon);
export const Plus = wrap(N.PlusIcon);
export const Search = wrap(N.SearchIcon);
export const Settings = wrap(N.SettingsIcon);
export const Stop = wrap(N.StopIcon);
export const Trash = wrap(N.TrashIcon);
export const User = wrap(N.UserIcon);

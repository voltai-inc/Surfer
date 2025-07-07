# Changelog

All notable changes to this project will be documented in this file.

Surfer is currently unstable and all 0.x releases are expected to contain
breaking changes. Releases are mainly symbolic and are done on a six-week
release cycle. Every six weeks, the current master branch is tagged and
released as a new version.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/).

## [0.3.0] - 2024-12-20

## Added

- Bumped backend to Wellen 0.13.6
- MIPS translator.
- RV32 and RV64 translators support all unprivileged instructions.
- Parameters now have an icon in the variable list and are drawn in a separate color, `variable_parameter` in the config.
- Custom instruction decoders can be loaded from the config directory.
- It is possible to press tab to expand text in the command prompt.
- Loading and saving states from within the ui has been added/improved.
- Separate server binary, `surver`.
- A number of color-blind friendly themes.
- [FTR](https://github.com/Minres/LWTR4SC) transaction streams are now supported.
- It is now possible to configure the possible UI zoom levels, `zoom_factors` and the default choice, `default_zoom_factor`.
- A link to a web page with all licenses to dependencies is added in the License information dialog.
- Initial [user](https://docs.surfer-project.org/book/) and [API](https://docs.surfer-project.org/surfer/) documentation.
- New configuration parameters `waveforms_line_height` and `waveforms_text_size` to control the height and text size of waveforms, respectively.
- It is now possible to add variables by dragging scopes and variables from the sidebar.
- Add `waves_loaded`, `index_of_name` and `spade_loaded`  to the wasm API
- Add `ViewportStrategy` which allows programmatic smooth scroll when scripting Surfer
- Add `ExpandItem` message which expands the fields of a viewed variable.
- Add `SetConfigFromString` which allows setting a configuration when Surfer is embedded in a webpage.
- `scope_add_recursive` command.
- Dialog will show up when a file is changed on disk, asking for reload. Not yet working on Windows.
- Translators for number of ones, leading/trailing ones/zeros and identical MSBs (sign-bits).
- The mouse gestures can be accessed through ctrl/cmd and primary mouse button (for use, e.g., when no middle mouse button is available).
- Show start and end time of mouse gesture zoom
- Allow mouse gesture zoom with ctrl+left click
- Add a timeline by default

## Changed

- Limit scrolling to always show some of the waveform
- Text color is (often) selected based on highest contrast vs background color. It is selected as one of the two config values `foreground` and `alt_text_color`.
- BREAKING: the `ticks` settings are moved from config to theme.
- Respect direction of arrows in `DrawTextArrow`
- Empty scopes are not shown by default, can be enabled by setting `show_empty_scopes` to `true` or using the menu entry.
- Parameters are shown in the scope list rather than among the variables, can be moved to variables by setting `show_parameters_in_scopes` to `false` or using the menu entry.
- The zoom in mouse gesture now shows the duration of the zoomed region.
- Variables are now sorted when added with `scope_add`

## Fixed

- Crash related to signals not being assigned values at time zero and snapping.
- Loading VCD files with errors more rarely crashes Surfer. In addition, the log window now pops up in case of an error.
- Empty scopes are no longer expandable in the tree hierarchy.
- The server can now be accessed in the web/WASM version.
- Translator selection is now deterministic. Earlier, different translators may be selected if more than one was preferred, notably this happened for single-bit enums.
- When variables are added using the `scope_add` command, they are sorted so that the result is identical to selecting the scope and pressing the `+` button.
- Variables with negative indices are now correctly sorted.
- Remove lingering marker line when deleting a marker

## Removed

## Other

- egui 0.30 is now used. This changes the shadowing in the UI and fixes an issue with scaling the UI in web browsers.

## [0.2.0] - 2024-05-31

## Added
- It is possible to disable the performance plot, by disabling the feature `performance_plot`, reducing the binary size with about 250kB.
- Clicking in the overview widget will now center the (primary, see multiple viewports) view to that position. It is also possible to drag the mouse while pressed.
- Allow injecting [Messages](https://gitlab.com/surfer-project/surfer/-/blob/main/src/message.rs?ref_type=heads#L27) into Surfer via `window.inject_message` in Javascript. Note that the Message enum, at least for now, may change at any time.
- Added some commands to the command prompt which were available only in the GUI before.
- Added a context menu where cursors can be added.
- Added an alternative tree-like hierarchy view to the left sidebar.
- Added an alternative text color config, currently used for marker boxes, `alt_text_color`.
- Multiple viewports are now supported. These can be added with `viewport_add`. The separator is configurable using the `viewport_separator` config value.
- Added jump to next and previous transition.
- The value of the selected variable at the cursor can now be copied to the clipboard, either using the toolbar, variable context menu, or standard keyboard short cut Ctrl/Cmd+c. Using the `copy_value` command, the value of any variable can be copied.
- There is now experimental support for GHW-files.
- Added a license window to display the license text for Surfer. Licenses for used crates are missing though.
- Added enum translator.
- The variable name filtering can now be case insensitive.
- Auto time scale that selects the largest time unit possible for each tick without having to reside to fractional numbers.
- Themes can be changed using the `view/theme` GUI options and with a command.
- It is now possible to drag-and-drop reorder variables in the waveform view.
- There is now a pre-built binary for macos-aarch64.
- Added an experimental client-server approach for running surfer at a remote location. Start with `surfer server --file=waveformfile.vcd/fst/ghw` where the file exists and follow the instructions in the output.
- Added undo/redo functionality
- The port direction is (optionally, but on by default) shown for variables in the variable list.
- New RISC-V instruction decoder with support for RV32IMAFD.

## Changed

- Renamed `cursors` to `markers` to differentiate the named and numbered *markers* from the *cursor* that moves with clicks.
- egui is updated to version 0.25.
- Icons are changed from Material Design to Remix Icons.
- Display scopes and variables while loading the variable change data and bring back progress bar when loading
- Translators that do not match the required word length for a variable are now not removed, but put in the "Not recommended" submenu. While there is often no reason to select a not recommended translator, the change leads to that variables that changes word length are not removed during a reload.
- The progress bar when loading waveforms has been moved to the status bar.

## Fixed
- Ticks do not longer disappear or become blurry at certain zoom levels.
- Files that do not initialize variables at time 0 now works.
- Transitions are no longer drawn for false transitions, e.g., from 0 to 0.
- Fixed anti-aliasing for variables which are mostly 1.
- The alternate background now fully covers the variable value column.
- Top-level variables that are not in a scope are now visible.
- The Cmd-key is now used on Mac (instead of Ctrl).
- Variable name filtering is faster (but adding lots of variables to the list view still takes a long time).
- Screen scaling has been improved so that it works on, e.g., HiDPI screens.
- Startup commands can now contain `load_waves` and friends.
- Copies of signals can now have different translators.
- Added rising edge markers to clock signals.

## Removed
- Buttons for adding divider and time at the bottom of the variable list is removed. Use the toolbar instead.

## Other
- There is now a VS Code [extension](https://marketplace.visualstudio.com/items?itemName=surfer-project.surfer) that loads the web-version of Surfer when opening a waveform file. [Repo](https://gitlab.com/surfer-project/vscode-extension).
- The minimum rustc version is determined and pinned to 1.75.


## [0.1.0] - 2023-03-07

Initial numbered version


[Unreleased]: https://gitlab.com/surfer-project/surfer/-/compare/v0.3.0...main
[0.3.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.3.0
[0.2.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.2.0
[0.1.0]: https://gitlab.com/surfer-project/surfer/-/tree/v0.1.0

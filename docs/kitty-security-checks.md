# Kitty security checks

[Return to the README](../README.md).

Manual testing on macOS with Kitty 0.43.1 confirmed the allowed commands, denial of `get-colors`, and Pi Watch navigation. Test your own version before you rely on these restrictions.

## Test a separate instance

Test in a separate instance before you change your normal setup. The instance runs under your account, not in a security sandbox. This procedure does not change your normal Kitty configuration.

1. In your existing shell, create a temporary private directory:

   ```sh
   test_dir="$(mktemp -d /tmp/pi-watch-kitty.XXXXXX)"
   chmod 700 "$test_dir"
   ls -ld "$test_dir"
   ```

   Confirm that you own the directory and its permissions are `drwx------`.

2. Start a separate Kitty process without your normal configuration:

   ```sh
   kitty --config NONE \
     --listen-on "unix:$test_dir/control.sock" \
     -o 'allow_remote_control=password' \
     -o 'remote_control_password="" ls focus-window' &
   ```

   If `kitty` is not on `PATH`, use its absolute executable path. On macOS, the usual path is `/Applications/kitty.app/Contents/MacOS/kitty`.

3. Inside the new window, check the permitted commands:

   ```sh
   kitty @ --to "$KITTY_LISTEN_ON" --use-password never \
     ls --match "id:$KITTY_WINDOW_ID"
   kitty @ --to "$KITTY_LISTEN_ON" --use-password never \
     focus-window --match "id:$KITTY_WINDOW_ID"
   ```

   Both commands must succeed without a prompt. The first returns pane metadata. Do not share that output without review. The second focuses the current pane, so no visible change is necessary. `--use-password never` prevents these checks from using an existing password source.

4. In the same window, check a harmless read-only command that the rule must deny:

   ```sh
   kitty @ --to "$KITTY_LISTEN_ON" --use-password never get-colors
   printf 'Exit status: %s\n' "$?"
   ```

   Expect an authorization error and a nonzero exit status. Stop if the command succeeds or requests approval.

5. Open a second pane in the test instance. Load Pi Watch in Pi sessions in both panes. Confirm that completion markers appear and Ctrl+' switches panes. This checks Pi Watch's normal invocation, which lets Kitty's CLI use its default password sources.

6. Close the test instance manually. In the original shell, remove the temporary directory:

   ```sh
   rm -r -- "$test_dir"
   unset test_dir
   ```

## Check your normal setup

After you apply the [README setup](../README.md#usage), start a new Kitty instance with that configuration. Repeat the allowed-command and denied-command checks from steps 3 and 4 inside that instance. Check Pi Watch with two panes as described in step 5.

Verify socket directory ownership and mode `0700`. Review other passwords, prior approvals, and per-window grants for broader access. The denied-command check covers one command without credentials. It does not prove that every access path is restricted.

Verify the restrictions again after any Kitty configuration change. Removing Pi Watch does not disable Kitty remote control. Private directory permissions do not block processes under your account.

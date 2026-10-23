# Every surface, as it is

What a person can see in the Android app and the desktop window: every page, pop-up, message and notification, with its words as the code has them. Made on 2026-10-24 from the code at `e5d1ad1` by `scripts/design-inventory.py`, which reads the source and writes this file again, as the checklist for the redesign the owner asked for on 2026-10-20:

> can u run figma, i would like to redesign our application again with your help, i want to go through every page, pop up, text, notfication

Each item is numbered so that it can be ticked off: **A** for Android, **D** for the desktop window. Within a place, *page* is what is on the screen itself, *pop-up* is a sheet, dialog or panel, *message* is a short line that appears and goes (a snackbar or toast) or an error, and *notification* is the system's. Each line gives the words and where they are in the code.

## Known gaps

This list was made by a script, so:

- **Words put together in code come out in pieces.** A sentence built from parts, such as a device's name and a count, shows as its fixed parts, with `${...}` or `{...}` where a value goes. Some sentences are split across several lines.
- **A pop-up is recognised by the code that opens it.** Words in the same function as a sheet are listed under it, even when some of them belong to the page.
- **A message is recognised by the line that shows it.** Words kept in a variable and shown a line or two later are filed as page text: *Could not settle that* on Android's Home is one.
- **What Android or the desktop draws for itself is left out**: the system's permission prompts, the file picker, the share sheet's own rows, and the screen-lock prompt apart from its title.
- **Shown only by the engine in rare cases**: the messages from the engine are listed, but when each one appears has not been worked out for every one.
- **Icons, illustrations and motion are not listed.** This is words and surfaces only.

## Android

### A1. Setup

**A1.1** page — `SetupActivity.kt`, `onCreate`

- Keep the key this phone made when you started, and use it as your first device. <sub>72</sub>

**A1.2** page — `SetupActivity.kt`, `create`

- Could not set up <sub>105</sub>

**A1.3** page — `SetupActivity.kt`, `join`

- Could not join <sub>121</sub>

**A1.4** page — `SetupActivity.kt`, `restore`

- That phrase was not accepted <sub>146</sub>

**A1.5** page — `SetupActivity.kt`, `fromBackup`

- Could not use the backed-up key <sub>160</sub>
- There is no key in your Google backup any more. <sub>161</sub>

**A1.6** page — `activity_setup.xml`, `activity_setup`

- Qurb <sub>38</sub>
- Your files stay on your own devices. They move directly between them, encrypted end to end, and are never stored on anyone else's server. <sub>45</sub>
- Set up Qurb here <sub>91</sub>
- This phone becomes your first device. There is nothing to write down. <sub>99</sub>
- I already use Qurb <sub>148</sub>
- Join the devices you already have, with the code one of them shows. <sub>155</sub>
- Use the key from your Google backup <sub>179</sub>
- Scan the code from your other device <sub>196</sub>
- On your computer, open Qurb, go to Devices and choose Add a device. Scan the code it shows, or type its six digits on the same Wi-Fi. It brings your key with it, so there is nothing else to type. <sub>203</sub>
- Scan the code <sub>211</sub>
- Or type its six digits <sub>219</sub>
- Back <sub>246</sub>
- Join <sub>254</sub>
- Use my 24 words instead <sub>265</sub>
- Enter your 24 words <sub>280</sub>
- The 24 words, separated by spaces <sub>286</sub>
- Restore <sub>317</sub>

### A2. Home

**A2.1** page — `HomeScreen.kt`, `refresh`

- Qurb can't read this phone's files <sub>95</sub>

**A2.2** page — `HomeScreen.kt`, `show`

- Add your first device <sub>112</sub>
- Qurb keeps your files on devices you own. Connect your computer or another phone to begin. <sub>113</sub>
- Bringing your devices up to date. <sub>115</sub>
- Syncing… <sub>115</sub>
- 1 file is waiting to reach your devices <sub>118</sub>
- ${waiting.size} files are waiting to reach your devices <sub>119</sub>
- Backing up to $keeper <sub>127</sub>
- ${Words.files(backupsLeft(state))} still to go. It carries on whenever $keeper is on. <sub>128</sub>
- Waiting for $to to collect <sub>132</sub>
- 1 file sent, not collected yet. <sub>133</sub>
- ${state.sends.size} files sent, not collected yet. <sub>134</sub>
- Not synced yet <sub>136</sub>
- Sync to bring your devices up to date. <sub>136</sub>
- Everything is synced. <sub>138</sub>
- Your files are safe. ${if (state.conflicts.size == 1)  <sub>140</sub>
- } your attention. <sub>140</sub>
- Your files are safe. Nothing needs your attention. <sub>142</sub>
- Add a device <sub>148</sub>
- Send <sub>152</sub>
- ${Words.size(state.usage.logical)} of files <sub>160</sub>
- synced ${Words.ago(it)} <sub>162</sub>

**A2.3** page — `HomeScreen.kt`, `showWaiting`

- Waiting <sub>180</sub>
- Not on $going yet  ·  ${Words.size(f.size)} <sub>191</sub>
- ${files.size - WAITING_ROWS} more files <sub>194</sub>
- ${Words.size(files.drop(WAITING_ROWS).sumOf { it.size })} not on $going yet <sub>195</sub>
- Sent to ${w.to}, not collected yet  ·  ${Words.size(w.size)} <sub>199</sub>
- $more more sends <sub>203</sub>
- 1 more send <sub>203</sub>
- Not collected yet <sub>204</sub>
- WhatsApp backup <sub>208</sub>
- ${Words.files((whatsApp.files - whatsApp.heldFiles).toInt())} still to go to $keeper <sub>209</sub>
- Your folders <sub>213</sub>
- ${Words.files((folders.files - folders.heldFiles).toInt())} still to go to $keeper <sub>214</sub>
- Everything here has reached $going. <sub>217</sub>
- Nothing waiting <sub>217</sub>

**A2.4** page — `HomeScreen.kt`, `showBackups`

- Backups <sub>236</sub>
- Your folders <sub>242</sub>
- Home <sub>243</sub>

**A2.5** page — `HomeScreen.kt`, `onlyHereSays`

- They're in your Private Vault, and no device is keeping a backup yet. Choose one in Devices. <sub>255</sub>
- Until then they're only on this phone. They go to $to the next time both are online. <sub>259</sub>
- Until then they're only on this phone. They go to $to, each the next time it and this phone are online. <sub>260</sub>

**A2.6** page — `HomeScreen.kt`, `hero`

- Everything is synced. <sub>295</sub>

**A2.7** page — `HomeScreen.kt`, `showConflicts`

- ${conflicts.size} things need attention <sub>310</sub>
- 1 thing needs attention <sub>310</sub>
- ${conflicts[0].path.substringAfterLast('/')} has two versions. <sub>312</sub>
- ${conflicts.size} files have two versions. <sub>314</sub>
- Review <sub>316</sub>

**A2.8** page — `HomeScreen.kt`, `settle`

- Could not settle that <sub>390</sub>

**A2.9** page — `screen_home.xml`, `screen_home`

- Qurb <sub>43</sub>
- Activity <sub>52</sub>
- Sync now <sub>114</sub>
- Send <sub>124</sub>

**A2.10** pop-up: “Two versions” — `HomeScreen.kt`, `choose`

- Two versions <sub>322</sub>
- Both are kept. Nothing is lost whichever you choose. <sub>323</sub>

**A2.11** pop-up: “Two devices changed it at the same time” — `HomeScreen.kt`, `describe`

- Since deleted or renamed <sub>335</sub>
-  ¶ Not on this phone yet <sub>337</sub>
- Two devices changed it at the same time <sub>339</sub>
- This version <sub>344</sub>
- The other version <sub>345</sub>
- Whichever you don't keep goes to Recently deleted for 30 days. <sub>371</sub>
- The other version isn't on this phone yet, so only this one can be kept for now. <sub>374</sub>
- Keep this version <sub>375</sub>
- Keep the other version <sub>377</sub>
- Keep both <sub>378</sub>

**A2.12** message — `HomeScreen.kt`, `settle`

- Kept ${kept.substringAfterLast('/')} <sub>387</sub>
- Kept both <sub>387</sub>

### A3. Files and Private Vault

**A3.1** page — `FilesScreen.kt`, `(top level)`

- Largest first <sub>56</sub>
- Name <sub>56</sub>
- Newest first <sub>56</sub>
- Private Vault <sub>67</sub>
- Private to this phone. Only this phone can see these files;  <sub>68</sub>
- a device you choose can keep a backup. <sub>69</sub>
- Search Private Vault <sub>70</sub>
- Files <sub>73</sub>
- Everything in your Qurb space, and where it is. <sub>76</sub>

**A3.2** page — `FilesScreen.kt`, `refresh`

- Your folder, kept on $it and sealed on this phone: nobody using $it can open it.  <sub>162</sub>
- Each file comes back when you open it. <sub>163</sub>
- Private to this phone. Only this phone can see these files; a device you choose can keep a backup. <sub>164</sub>
- Private Vault <sub>170</sub>
- Private to this phone <sub>171</sub>
- Your folder on ${keeper.name} <sub>175</sub>
- Backups, WhatsApp, deleted files · while it's on <sub>176</sub>
- Folders <sub>179</sub>
- Files <sub>181</sub>
- Results <sub>181</sub>
- Recently deleted <sub>184</sub>
- Kept for 30 days <sub>185</sub>
- Could not read this phone's files <sub>191</sub>

**A3.3** page — `FilesScreen.kt`, `showEmpty`

- Nothing called “$query”. <sub>203</sub>
- This folder is empty. <sub>204</sub>
- Nothing here yet. Files you add here stay on this phone. <sub>206</sub>
- Nothing here yet. Add files, or add a device and sync to see what it shares. <sub>208</sub>

**A3.4** page — `FilesScreen.kt`, `drawCrumbs`

- Private Vault <sub>228</sub>
- Qurb <sub>228</sub>

**A3.5** page — `FilesScreen.kt`, `rename`

- Rename <sub>334</sub>

**A3.6** page — `FilesScreen.kt`, `move`

- Move <sub>341</sub>
- Move to folder <sub>341</sub>

**A3.7** page — `FilesScreen.kt`, `moveTo`

- Could not do that <sub>353</sub>

**A3.8** page — `FilesScreen.kt`, `newFolder`

- Make <sub>360</sub>
- New folder <sub>360</sub>
- Could not make that folder <sub>367</sub>

**A3.9** page — `FilesScreen.kt`, `free`

- Could not free that <sub>385</sub>

**A3.10** page — `FilesScreen.kt`, `fetch`

- Could not ask for that <sub>443</sub>

**A3.11** page — `screen_files.xml`, `screen_files`

- Files <sub>36</sub>
- More <sub>62</sub>
- Search files <sub>81</sub>
- Add files <sub>154</sub>

**A3.12** pop-up — `FilesScreen.kt`, `more`

- Sort: ${s.label} <sub>249</sub>
- New folder… <sub>254</sub>
- Save ${Words.files(here.size)} to this phone… <sub>257</sub>
- Recently deleted <sub>259</sub>

**A3.13** pop-up — `FilesScreen.kt`, `choose`

- Private Vault <sub>271</sub>
- Qurb <sub>271</sub>
- On this phone, and another device has it too. <sub>276</sub>
- This is the only copy currently stored in Qurb. It reaches your other devices the next time one is online. <sub>277</sub>
- Coming to this phone at the next sync. <sub>278</sub>
- Not on this phone right now. It stays in Qurb; keep it here to open it. <sub>279</sub>
- No device this phone syncs with has this file. It was freed while another  <sub>280</sub>
- device kept it, and that device no longer has it or is no longer paired. It stays listed so  <sub>281</sub>
- you know it existed; deleting it removes it from your devices' lists. <sub>282</sub>
- Changed ${Words.ago(entry.modifiedAt / 1_000_000_000)} <sub>285</sub>
- Open <sub>292</sub>
- Keep on this phone <sub>294</sub>
- Free local space <sub>297</sub>
- Send to device… <sub>300</sub>
- Send $name to <sub>301</sub>
- Save a copy to this phone <sub>303</sub>
- Rename… <sub>304</sub>
- Move to folder… <sub>305</sub>
- Move to Files, on all your devices <sub>308</sub>
- Move to Private Vault… <sub>310</sub>
- Delete <sub>313</sub>

**A3.14** pop-up: “Move $name to Private Vault?” — `FilesScreen.kt`, `moveArea`

- Could not move that <sub>406</sub>
- No device keeps a backup of your Private Vault, so this phone will have the only copy.  <sub>418</sub>
- Choose one in Devices to keep it. <sub>419</sub>
- ${keepers[0].name} keeps a backup of your Private Vault, so it keeps this file too. <sub>421</sub>
- ${Words.list(keepers.map { it.name })} keep a backup of your Private Vault, so they keep this file too. <sub>422</sub>
- Move $name to Private Vault? <sub>425</sub>
- It stays on this phone, and your other devices remove their copies at their next sync  <sub>426</sub>
- — each keeps it in Recently deleted for 30 days, as with any deletion. $kept <sub>427</sub>
- Move <sub>428</sub>

**A3.15** pop-up: “Delete ${entry.path.substringAfterLast('/')}?” — `FilesScreen.kt`, `delete`

- It's deleted from this phone, and from any device keeping a backup of your Private Vault. <sub>450</sub>
- It's in your Qurb space, so it's deleted on every device at its next sync. <sub>452</sub>
- Delete ${entry.path.substringAfterLast('/')}? <sub>455</sub>
- $where Recently deleted keeps it for 30 days. <sub>456</sub>
- Delete <sub>457</sub>
- Could not delete that <sub>464</sub>

**A3.16** message — `FilesScreen.kt`, `moveTo`

- Now ${to.substringAfterLast('/')} <sub>350</sub>

**A3.17** message — `FilesScreen.kt`, `free`

- Freed ${Words.size(freed)}. ${entry.path.substringAfterLast('/')} stays in Qurb. <sub>383</sub>

**A3.18** message — `FilesScreen.kt`, `moveArea`

- $name goes to all your devices <sub>403</sub>
- $name is in Private Vault <sub>403</sub>

**A3.19** message — `FilesScreen.kt`, `delete`

- Deleted. It's in Recently deleted for 30 days. <sub>461</sub>

### A4. Recently deleted

**A4.1** page — `DeletedScreen.kt`, `(top level)`

- Files <sub>26</sub>
- Settings <sub>26</sub>
- Recently deleted <sub>28</sub>
- Files deleted on any of your devices are kept for 30 days. Restoring one puts it back where it was. <sub>29</sub>

**A4.2** page — `DeletedScreen.kt`, `refresh`

- Nothing deleted recently. <sub>41</sub>
-  on $it <sub>44</sub>
- Private Vault  ·   <sub>45</sub>
- $vault${Words.ago(d.deletedAt)}$by  ·   <sub>51</sub>
- $days day${if (days == 1L)  <sub>52</sub>
- last day <sub>52</sub>
- } left <sub>52</sub>
- Restore <sub>53</sub>
- Could not read Recently deleted <sub>59</sub>

**A4.3** page — `DeletedScreen.kt`, `restore`

- Restored as ${at.substringAfterLast('/')} <sub>82</sub>
- Restored to your Private Vault <sub>83</sub>
- Restored, on all your devices <sub>84</sub>
- Could not restore it <sub>88</sub>

**A4.4** page — `DeletedScreen.kt`, `forget`

- Could not delete it <sub>101</sub>

**A4.5** pop-up: “${Words.size(d.size)}  ·  deleted ${Words.ago(d.deletedAt)}” — `DeletedScreen.kt`, `choose`

- ${Words.size(d.size)}  ·  deleted ${Words.ago(d.deletedAt)} <sub>68</sub>
- Restoring puts it back in your Private Vault. <sub>69</sub>
- Restoring puts it back in Qurb, and it returns on your other devices at their next sync. <sub>70</sub>
-  ¶  ¶ ${it.replaceFirstChar(Char::uppercase)}. <sub>71</sub>
- Restore <sub>72</sub>
- Delete for good <sub>73</sub>

**A4.6** message — `DeletedScreen.kt`, `forget`

- Deleted for good <sub>99</sub>

### A5. Your folders

**A5.1** page — `FoldersScreen.kt`, `(top level)`

- Your folders <sub>31</sub>

**A5.2** page — `FoldersScreen.kt`, `refresh`

- Could not read this phone's folders <sub>57</sub>

**A5.3** page — `FoldersScreen.kt`, `show`

- Backed up to the computer that keeps your Private Vault. <sub>68</sub>
- Backed up to $keeper over Wi-Fi. What you delete here stays there. <sub>69</sub>
- No computer keeps this phone's Private Vault yet. Choose one in Devices, and its folders back up to it. <sub>73</sub>
- Devices <sub>74</sub>
- To back up its folders, Qurb needs to read this phone's files. It reads only the folders you  <sub>79</sub>
- choose, and lists the rest's names and sizes for $keeper. <sub>80</sub>
- Allow <sub>81</sub>
- On $keeper <sub>89</sub>
- Nothing yet. It goes over Wi-Fi. <sub>90</sub>
- ${Words.count(state.backup.heldFiles)} of ${Words.files(state.backup.files.toInt())},  <sub>91</sub>
- ${Words.size(state.backup.heldBytes)} of ${Words.size(state.backup.bytes)} <sub>92</sub>
- ${Words.files(state.backup.onlyKept.toInt())}, kept on $keeper <sub>94</sub>
- Deleted on this phone <sub>94</sub>
- Back up now <sub>96</sub>
- On Wi-Fi <sub>96</sub>
- Looking at this phone's folders. Pull down in a moment. <sub>103</sub>

**A5.4** page — `FoldersScreen.kt`, `addSwitch`

-  · ${folder.keptFiles} on $keeper <sub>123</sub>
- Backed up with ${outer.path} <sub>124</sub>

**A5.5** page — `FoldersScreen.kt`, `choose`

- Could not choose $path <sub>144</sub>

**A5.6** message — `FoldersScreen.kt`, `show`

- Backing up on Wi-Fi <sub>98</sub>

**A5.7** message — `FoldersScreen.kt`, `choose`

- $path backs up nothing new <sub>141</sub>
- $path backs up to $keeper on Wi-Fi <sub>141</sub>

### A6. Backups: WhatsApp and folders

**A6.1** page — `Backups.kt`, `foldersLine`

- Off. Photos, documents, any folder, kept on ${keeper ?:  <sub>18</sub>
- Choose which, here or on ${keeper ?:  <sub>19</sub>
- ${Words.folders(chosen)}: starting, on Wi-Fi <sub>20</sub>
- ${Words.folders(chosen)}, all ${Words.files(backup.files.toInt())}, on $keeper <sub>22</sub>
- ${Words.folders(chosen)}: ${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())}  <sub>23</sub>
- on ${keeper ?:  <sub>24</sub>
- } so far <sub>24</sub>

**A6.2** page — `Backups.kt`, `whatsAppLine`

- Off. Keeps WhatsApp's chats and media on ${keeper ?:  <sub>28</sub>
- instead of in your Google account <sub>29</sub>
- Starting: it runs on Wi-Fi <sub>30</sub>
- All ${Words.files(backup.files.toInt())}, ${Words.size(backup.bytes)}, on $keeper <sub>32</sub>
- ${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())} on ${keeper ?:  <sub>33</sub>
- } so far <sub>33</sub>

**A6.3** page — `WhatsApp.kt`, `run`

- ${run.added + run.changed + run.unchanged} files <sub>132</sub>
- ${run.added} new <sub>133</sub>
- ${run.changed} changed <sub>134</sub>
- ${run.kept} deleted in WhatsApp, kept <sub>135</sub>
- ${run.remaining} left for next time <sub>136</sub>
- ${run.failed} could not be read <sub>137</sub>
- ${run.added + run.changed} files in ${took} ms; $run <sub>144</sub>

**A6.4** pop-up: “Back up WhatsApp” — `Backups.kt`, `whatsApp`

- Back up WhatsApp <sub>39</sub>
- To $it <sub>40</sub>
- No computer keeps this phone's Private Vault yet. Choose one in Devices:  <sub>42</sub>
- the backup goes to it. <sub>43</sub>
- Devices <sub>44</sub>
- WhatsApp's chats, photos, videos and documents, kept on $keeper in this phone's  <sub>48</sub>
- Private Vault: out of sight there, and nothing copied on this phone. Photos and videos  <sub>49</sub>
- you delete in WhatsApp stay on $keeper. <sub>50</sub>
- Over Wi-Fi only. The first time is everything WhatsApp keeps; after that,  <sub>51</sub>
- what changed each night. <sub>52</sub>
- Restoring WhatsApp from it hasn't been tried yet, so keep WhatsApp's own Google  <sub>55</sub>
- backup on for now. Turning off Include videos there saves the most space. <sub>56</sub>
- Next, Android opens WhatsApp's folder: choose Use this folder, then Allow. <sub>57</sub>
- Choose folder <sub>60</sub>
- Not now <sub>60</sub>
- WhatsApp backup <sub>65</sub>
- No computer keeps it yet <sub>66</sub>
- On $it <sub>66</sub>
- On ${keeper ?:  <sub>69</sub>
- Nothing yet <sub>70</sub>
- ${Words.count(backup.heldFiles)} of ${Words.files(backup.files.toInt())},  <sub>71</sub>
- ${Words.size(backup.heldBytes)} of ${Words.size(backup.bytes)} <sub>72</sub>
- ${Words.files(backup.onlyKept.toInt())}, kept on $keeper <sub>74</sub>
- Deleted in WhatsApp <sub>74</sub>
- Last run <sub>76</sub>
- Not yet. It runs on Wi-Fi. <sub>77</sub>
- Restoring WhatsApp from it hasn't been tried yet: keep WhatsApp's own Google backup on  <sub>79</sub>
- for now. <sub>80</sub>
- Back up now <sub>81</sub>
- On Wi-Fi <sub>81</sub>
- Stop backing up… <sub>85</sub>

**A6.5** pop-up: “Stop backing up WhatsApp?” — `Backups.kt`, `stopWhatsApp`

- Stop backing up WhatsApp? <sub>90</sub>
- Nothing new goes to ${keeper ?:  <sub>91</sub>
- }. What it keeps already stays there. <sub>91</sub>
- Stop <sub>92</sub>

**A6.6** message — `Backups.kt`, `whatsApp`

- Backing up WhatsApp on Wi-Fi <sub>83</sub>

### A7. A phone's folder on a computer

**A7.1** page — `KeptScreen.kt`, `(top level)`

- Files <sub>39</sub>
- On ${computer.name} <sub>41</sub>

**A7.2** page — `KeptScreen.kt`, `refresh`

- Your folder there: backups, WhatsApp and Private Vault,  <sub>63</sub>
- deleted files included, while ${computer.name} is on. <sub>64</sub>
- Reaching ${computer.name}… <sub>67</sub>
- ${computer.name} isn't reachable. It has to be on with Qurb running, and this phone  <sub>85</sub>
- Try again <sub>86</sub>
- online. <sub>86</sub>

**A7.3** page — `KeptScreen.kt`, `show`

- ${computer.name} keeps nothing for this phone yet. <sub>99</sub>
- This folder is empty. <sub>99</sub>
- Folders <sub>104</sub>
- Phones <sub>104</sub>
- Files <sub>112</sub>

**A7.4** page — `KeptScreen.kt`, `showFiles`

- Show ${minOf(PAGE, files.size - upTo)} more <sub>130</sub>
- ${Words.count(upTo.toLong())} of ${Words.files(files.size)} shown <sub>131</sub>

**A7.5** page — `KeptScreen.kt`, `open`

- the folder is closed <sub>179</sub>
- Open with <sub>186</sub>
- Nothing on this phone can open ${file.name} <sub>188</sub>
- Could not open ${file.name} <sub>190</sub>

**A7.6** page — `KeptScreen.kt`, `restoredTo`

- Back into your Private Vault <sub>242</sub>

**A7.7** page — `KeptScreen.kt`, `restore`

- Restored <sub>256</sub>
- the folder is closed <sub>263</sub>
- Could not restore ${file.name} <sub>273</sub>

**A7.8** page — `KeptScreen.kt`, `fetch`

- the folder is closed <sub>284</sub>
- Could not fetch ${file.name} <sub>289</sub>

**A7.9** pop-up — `KeptScreen.kt`, `choose`

- In an app on this phone, without saving a copy <sub>148</sub>
- Open <sub>148</sub>
- A copy in this phone's Downloads, in Qurb <sub>151</sub>
- Download <sub>151</sub>
- Saved to Downloads <sub>152</sub>
- Restore <sub>154</sub>

**A7.10** message — `KeptScreen.kt`, `open`

- Fetching ${file.name}… <sub>197</sub>

**A7.11** message — `KeptScreen.kt`, `restore`

- Allow Qurb to manage this phone's files first, in Settings → Backups <sub>251</sub>
- Restored to your Private Vault <sub>270</sub>

**A7.12** message — `KeptScreen.kt`, `fetch`

- Fetching ${file.name}… <sub>280</sub>

### A8. Devices

**A8.1** page — `DevicesScreen.kt`, `(top level)`

- Devices <sub>34</sub>
- Your devices, and whether each is reachable. <sub>35</sub>
- Add <sub>37</sub>

**A8.2** page — `DevicesScreen.kt`, `refresh`

- Could not read your devices <sub>57</sub>

**A8.3** page — `DevicesScreen.kt`, `show`

- This phone <sub>70</sub>
- Computers you visit as a guest <sub>79</sub>
- Add your computer or another phone, and your files move between them. On a computer,  <sub>85</sub>
- open Qurb, go to Devices and choose Add a device. <sub>86</sub>
- Add a device <sub>87</sub>

**A8.4** page — `DevicesScreen.kt`, `deviceCard`

- You visit as a guest <sub>94</sub>
- Keeps a backup of your Private Vault <sub>95</sub>

**A8.5** page — `DevicesScreen.kt`, `remove`

- Could not remove it <sub>278</sub>

**A8.6** page — `DevicesScreen.kt`, `setKeeping`

- Could not change that <sub>296</sub>

**A8.7** pop-up — `DevicesScreen.kt`, `open`

- Someone else's computer, which this phone visits as a guest. It sees only what you send it. <sub>163</sub>
- Who <sub>163</sub>
- Keep my files here <sub>164</sub>
- Your Private Vault is kept on ${peer.name}, sealed: nobody using it can open your files.  <sub>165</sub>
- Each one comes back when you open it. <sub>166</sub>
- Your Private Vault, kept on ${peer.name} and sealed so nobody there can open it. This phone then keeps none of its own copies. <sub>167</sub>
- Paired <sub>172</sub>
- Send files… <sub>174</sub>
- Stop visiting… <sub>175</sub>
- Identity ${peer.short} <sub>176</sub>
- Keep a backup of my Private Vault <sub>180</sub>
- ${peer.name} keeps a copy of what this phone keeps private, where nobody using it sees it. <sub>181</sub>
- Choose this, and this phone can free its own copies of private files without losing them. <sub>182</sub>
- Remove this device… <sub>190</sub>

**A8.8** pop-up: “Keep your Private Vault on ${peer.name}?” — `DevicesScreen.kt`, `keep`

- Keep your Private Vault on ${peer.name}? <sub>197</sub>
- ${peer.name} keeps a copy of the files this phone keeps private, starting at the next  <sub>198</sub>
- sync. Nobody using ${peer.name} sees them; they're yours, and come back to this phone  <sub>199</sub>
- when you ask. ¶  ¶ Once it has a file, this phone can free its own copy to save space. <sub>200</sub>
- Keep them there <sub>201</sub>

**A8.9** pop-up: “Keep your files on ${peer.name}?” — `DevicesScreen.kt`, `keepWithHost`

- Keep your files on ${peer.name}? <sub>208</sub>
- Your Private Vault goes to ${peer.name}, sealed on this phone first: whoever uses  <sub>209</sub>
- ${peer.name} can't open your files, or see their names. They see only how much space  <sub>210</sub>
- they take. ¶  ¶ Once ${peer.name} has a file, this phone lets go of its own copy, and  <sub>211</sub>
- fetches it back when you open it — while ${peer.name} is on. ¶  ¶  <sub>212</sub>
- It's their computer: they can delete what it keeps, though they can't read it. <sub>213</sub>
- Keep them there <sub>214</sub>

**A8.10** pop-up: “Stop keeping your files on ${peer.name}?” — `DevicesScreen.kt`, `stopKeeping`

- Stop keeping your files on ${peer.name}? <sub>220</sub>
- Nothing new goes to ${peer.name}. What it already has, it keeps: this phone doesn't  <sub>221</sub>
- reach into another device. ¶  ¶ A file this phone freed because ${peer.name} had it is still there. <sub>222</sub>
- Stop <sub>223</sub>

**A8.11** pop-up: “Remove ${peer.name}?” — `DevicesScreen.kt`, `askToRemove`

- Could not check what removing it would do <sub>237</sub>
- Remove ${peer.name}? <sub>240</sub>
- This phone stops trusting ${peer.name}: it can no longer connect or sync here.  <sub>241</sub>
- It keeps its key and everything already on it — removing it deletes nothing there. <sub>242</sub>
- ${peer.name} keeps a backup of your Private Vault. After this, nothing new goes  <sub>244</sub>
- there, and this phone can't get anything back from it. <sub>245</sub>
- ${Words.files(plan.onlyThere.size)} freed from this phone  <sub>248</sub>
- ${if (plan.onlyThere.size == 1)  <sub>249</sub>
- } kept only on ${peer.name}.  <sub>249</sub>
- Once it's removed, they can't be downloaded again. <sub>250</sub>
- ${Words.files(plan.waiting.toInt())} waiting for it to collect will be cancelled. <sub>253</sub>
- Also delete what this phone keeps for it <sub>261</sub>
- Only on this phone: your other devices go on trusting it until you remove it there too. <sub>265</sub>
- Remove device <sub>267</sub>

**A8.12** message — `DevicesScreen.kt`, `remove`

- ${peer.name} removed <sub>276</sub>

**A8.13** message — `DevicesScreen.kt`, `setKeeping`

- ${peer.name} keeps your Private Vault from the next sync <sub>291</sub>
- Stopped <sub>291</sub>

### A9. Add a device: show a code

**A9.1** page — `ShowCode.kt`, `show`

- Could not show a code <sub>39</sub>

**A9.2** pop-up: “It works once, for five minutes” — `ShowCode.kt`, `getOutline`

- Or read the whole code out, or type it: <sub>92</sub>
- It works once, for five minutes <sub>106</sub>
- Scan this on the other device <sub>106</sub>
- Stop showing it <sub>112</sub>
- Or type these on a device on this Wi-Fi <sub>124</sub>
- These were typed wrongly too often. Scan the code instead. <sub>125</sub>
- Waiting — this code works for ${left / 60}:${ <sub>128</sub>
- That code has expired. Close this and show a new one. <sub>130</sub>
- Not connected: ${e.message ?:  <sub>147</sub>

**A9.3** message — `ShowCode.kt`, `getOutline`

- Connected to ${peer.name} <sub>142</sub>

### A10. Add a device: scan a code

**A10.1** page — `ScanActivity.kt`, `(top level)`

- Camera permission refused. Go back and type the code instead. <sub>49</sub>

**A10.2** page — `ScanActivity.kt`, `start`

- Could not open the camera: ${e.message} <sub>78</sub>
- Could not start the camera: ${e.message} <sub>128</sub>

**A10.3** page — `activity_scan.xml`, `activity_scan`

- Point the camera at the code shown by `qurb pair` <sub>23</sub>

### A11. Pairing: approving a device

**A11.1** page — `Approval.kt`, `approve`

- wants to join, and take this phone's key <sub>39</sub>
- wants to pair with this phone <sub>40</sub>
- Approve only if it shows this number. <sub>44</sub>
- A different number means someone else has this code: decline. <sub>45</sub>
- Decline <sub>47</sub>
- Approve <sub>48</sub>

**A11.2** page — `Approval.kt`, `showWhileJoining`

- Approve this phone on the other device <sub>62</sub>
- It asks whether to let this phone in. Check it shows this number. <sub>64</sub>

### A12. Settings

**A12.1** page — `Appearance.kt`, `(top level)`

- As your phone is set <sub>26</sub>
- Light <sub>27</sub>
- Dark <sub>28</sub>

**A12.2** page — `SettingsScreen.kt`, `(top level)`

- Settings <sub>31</sub>
- How Qurb works on this phone. <sub>33</sub>

**A12.3** page — `SettingsScreen.kt`, `refresh`

- Could not read the settings <sub>74</sub>

**A12.4** page — `SettingsScreen.kt`, `show`

- This phone <sub>85</sub>
- ${android.os.Build.MODEL ?:  <sub>87</sub>
- Name <sub>87</sub>
- } — what your other devices call it <sub>87</sub>
- Key protection <sub>91</sub>
- In the Android Keystore: only Qurb on this phone can use it. It works while the phone  <sub>92</sub>
- is locked, so syncing in the background can too — your screen lock keeps others out of the app. <sub>93</sub>
- Devices <sub>95</sub>
- Your devices <sub>97</sub>
- None yet <sub>98</sub>
- Storage <sub>102</sub>
- Your files <sub>104</sub>
- Qurb on this phone <sub>105</sub>
- Who has each folder <sub>106</sub>
- Which devices each folder is on, and whether this phone keeps it <sub>107</sub>
- Recently deleted <sub>108</sub>
- Nothing. Files deleted on any device are kept 30 days <sub>109</sub>
- ${Words.files(state.deleted.size)}, ${Words.size(state.deleted.sumOf { it.size })} — restorable for 30 days <sub>110</sub>
- Clears what nothing needs any more <sub>113</sub>
- Free unused space <sub>113</sub>
- Copies of files you sent <sub>117</sub>
- ${Words.size(state.sentCopies)}, kept here after they arrived <sub>118</sub>
- Backups <sub>124</sub>
- Back up WhatsApp <sub>128</sub>
- Back up your folders <sub>131</sub>
- Settings <sub>132</sub>
- Privacy <sub>135</sub>
- Keep new files private <sub>138</sub>
- Files that arrive on this phone from other apps go to your Private Vault <sub>139</sub>
- Files that arrive on this phone from other apps go to all your devices <sub>140</sub>
- Notifications <sub>143</sub>
- On this phone <sub>147</sub>
- Off. What happens is still in Activity, from Home. <sub>149</sub>
- Android isn't letting Qurb notify yet: turn this on to allow it. <sub>150</sub>
- When somebody sends you a file, a device collects one you sent, or something fails.  <sub>151</sub>
- Nothing else. <sub>152</sub>
- Recovery <sub>160</sub>
- Checking… <sub>167</sub>
- Where your key is safe <sub>167</sub>
- Not kept for this phone yet: a new phone restored from this one's backup would  <sub>175</sub>
- not have it. Tap to keep it now. Any of your devices can also give it to a new one with a code. <sub>176</sub>
- On this phone, and in your Google backup, end-to-end encrypted with your screen lock  <sub>177</sub>
- — read back and checked just now. Any of your devices can also give it to a new one with a code. <sub>178</sub>
- On this phone only: with no screen lock, it is not backed up. Set one, or keep your  <sub>179</sub>
- computer paired: it can give the key to a new phone with a code. <sub>180</sub>
- Recovery phrase <sub>188</sub>
- The 24 words that spell your key, if you want them. Nobody needs to write them down. <sub>188</sub>
- Appearance <sub>192</sub>
- Theme <sub>194</sub>
- Full; follows this phone's animation settings <sub>195</sub>
- Motion <sub>195</sub>
- Reduced, as this phone's settings ask <sub>195</sub>
- Advanced <sub>197</sub>
- Background sync <sub>199</sub>
- Rendezvous service <sub>200</sub>
- Not set — only devices on this Wi-Fi <sub>201</sub>
- Relay <sub>202</sub>
- Beside your rendezvous service <sub>203</sub>
- None — devices must reach each other directly <sub>204</sub>
- Version <sub>210</sub>

**A12.5** page — `SettingsScreen.kt`, `setPrivate`

- Could not change that <sub>250</sub>

**A12.6** page — `SettingsScreen.kt`, `keepRemotely`

-  ${Words.files(r.kept.size)} stayed: this phone has the only copy. <sub>311</sub>
- Could not free that folder <sub>314</sub>

**A12.7** page — `SettingsScreen.kt`, `keepHere`

- Could not change that <sub>328</sub>

**A12.8** page — `SettingsScreen.kt`, `setSharing`

- $folder is on all your devices <sub>340</sub>
- Saved. A device left out keeps what it has, and gets nothing new. <sub>341</sub>
- Could not change that <sub>345</sub>

**A12.9** page — `SettingsScreen.kt`, `keepKey`

- Could not keep the key <sub>370</sub>

**A12.10** page — `SettingsScreen.kt`, `tidy`

- Could not free space <sub>383</sub>

**A12.11** pop-up: “Show your recovery phrase?” — `SettingsScreen.kt`, `warnThenShowPhrase`

- Show your recovery phrase? <sub>223</sub>
- Anyone who sees these 24 words can read every file you keep in Qurb, on any device.  <sub>224</sub>
- Make sure nobody is looking. <sub>225</sub>
- Show <sub>226</sub>
- Numbered, in order <sub>231</sub>
- Your recovery phrase <sub>231</sub>
- Close <sub>233</sub>
- Hide <sub>233</sub>
- Could not show the words <sub>237</sub>

**A12.12** pop-up: “Who has each folder” — `SettingsScreen.kt`, `chooseFolder`

- Could not read the folders <sub>266</sub>
- Who has each folder <sub>269</sub>
- A folder is on every device unless you choose <sub>270</sub>
- No folders yet. Folders appear here once they hold a file. <sub>271</sub>
- Every device <sub>273</sub>
- a removed device <sub>274</sub>
-   ·  downloaded when opened <sub>277</sub>

**A12.13** pop-up — `SettingsScreen.kt`, `folderSheet`

- A device you leave out keeps what it has, and gets nothing new <sub>285</sub>
- This phone <sub>290</sub>
- Free local space: download files when opened <sub>295</sub>
- Keep this folder on this phone <sub>295</sub>
- Save <sub>297</sub>

**A12.14** pop-up: “Theme” — `SettingsScreen.kt`, `chooseTheme`

- Theme <sub>353</sub>

**A12.15** pop-up: “Let go of ${Words.size(bytes)}?” — `SettingsScreen.kt`, `letGoOfSentCopies`

- Let go of ${Words.size(bytes)}? <sub>398</sub>
- Keep <sub>400</sub>
- Let go <sub>400</sub>
- Could not free that <sub>406</sub>

**A12.16** pop-up: “Background sync” — `SettingsScreen.kt`, `explainBackground`

- Background sync <sub>423</sub>
- Android decides when this runs. Fifteen minutes is the shortest period it accepts,  <sub>424</sub>
- and an idle phone may go much longer between attempts. With push, another device  <sub>425</sub>
- wakes this phone when it has something. ¶  ¶ Both devices have to be on at the same  <sub>426</sub>
- moment for a sync to happen, so a computer that is off is missed until next time. <sub>427</sub>
- Close <sub>428</sub>
- Run one now <sub>428</sub>

**A12.17** pop-up: “Relay” — `SettingsScreen.kt`, `editRelay`

- Relay <sub>458</sub>
- When this phone and another device can't reach each other directly — common on  <sub>459</sub>
- mobile data — their encrypted traffic goes through a relay instead. It can't read it. ¶  ¶  <sub>460</sub>
- Leave it empty to use the relay beside your rendezvous service, which is what  <sub>461</sub>
- most people want. Or enter the address of a relay of your own —  <sub>462</sub>
- relay.example.com:9001, or a wss:// address — or off for none. <sub>463</sub>
- Save <sub>466</sub>

**A12.18** pop-up: “Rendezvous service” — `SettingsScreen.kt`, `editSignal`

- Rendezvous service <sub>489</sub>
- Two devices find each other through this when they're not on the same network.  <sub>490</sub>
- Run `qurb signal` on a server of your own and enter the address it prints — wss://…  <sub>491</sub>
- for one reachable from anywhere. It never sees your files. <sub>492</sub>
- Save <sub>494</sub>

**A12.19** message — `SettingsScreen.kt`, `setPrivate`

- New files go to all your devices <sub>248</sub>
- New files stay private <sub>248</sub>

**A12.20** message — `SettingsScreen.kt`, `keepRemotely`

- Freed ${Words.size(r.bytes)}. Nothing was deleted.$kept <sub>312</sub>

**A12.21** message — `SettingsScreen.kt`, `keepHere`

- ${Words.files(asked.toInt())} on their way back <sub>325</sub>
- Kept here <sub>325</sub>

**A12.22** message — `SettingsScreen.kt`, `keepKey`

- Your key is kept <sub>368</sub>

**A12.23** message — `SettingsScreen.kt`, `tidy`

- Freed ${Words.size(tidied.freed)} <sub>381</sub>
- Nothing to free <sub>381</sub>

**A12.24** message — `SettingsScreen.kt`, `letGoOfSentCopies`

- Freed ${Words.size(freed)} <sub>404</sub>

**A12.25** message — `SettingsScreen.kt`, `explainBackground`

- Queued. It runs when Android allows. <sub>432</sub>

**A12.26** message — `SettingsScreen.kt`, `editRelay`

- Saved <sub>476</sub>
- Using the relay beside your rendezvous service <sub>476</sub>

**A12.27** message — `SettingsScreen.kt`, `editSignal`

- Saved <sub>500</sub>

### A13. Activity

**A13.1** page — `ActivityScreen.kt`, `(top level)`

- Home <sub>25</sub>
- Activity <sub>27</sub>
- Everything this phone did, newest first. <sub>28</sub>

**A13.2** page — `ActivityScreen.kt`, `refresh`

- Nothing has happened yet. <sub>40</sub>
- Show older <sub>46</sub>
- Could not read what happened <sub>52</sub>

### A14. The app frame: tabs, sending, asks, unlocking

**A14.1** page — `MainActivity.kt`, `heard`

- ${answer.name} didn't open your folder: its ask had lapsed, or it keeps nothing of yours yet. <sub>302</sub>

**A14.2** page — `MainActivity.kt`, `catchUp`

- Could not read this phone's files <sub>441</sub>

**A14.3** page — `MainActivity.kt`, `updateDock`

- Syncing with your devices… <sub>491</sub>
- ${waiting[0].path.substringAfterLast('/')} is waiting for ${waiting[0].to} <sub>504</sub>
- ${waiting.size} files waiting to be collected <sub>506</sub>

**A14.4** page — `MainActivity.kt`, `sync`

- Sending ${Words.size(forOthers)} to your devices. It carries on if you  <sub>657</sub>
- leave Qurb; a notification shows how far it has got. <sub>658</sub>
- Ran out of time before reaching a device. Try again. <sub>682</sub>
- No devices connected yet <sub>684</sub>
- No device answered. It has to be on with Qurb running, and this phone online. <sub>686</sub>
- Everything is synced <sub>688</sub>
- ${Words.files(outcome.adopted.toInt())} updated <sub>690</sub>
- , ${outcome.conflicts} with two versions <sub>692</sub>
-  — ran out of time, sync again <sub>694</sub>
- Sync failed <sub>700</sub>

**A14.5** page — `MainActivity.kt`, `sendGoes`

- Sent to ${to.name} <sub>744</sub>
- Waiting for ${to.name}. It collects it the next time it’s online — keep the file as it is until then. <sub>747</sub>
- Waiting for ${to.name}: $left of ${names.size} not collected yet. It gets them the next time it’s online — keep the files as they are until then. <sub>748</sub>

**A14.6** page — `MainActivity.kt`, `joinWith`

- Could not connect <sub>916</sub>

**A14.7** page — `MainActivity.kt`, `addFiles`

- Added $added, then stopped <sub>946</sub>
- Could not add that file <sub>946</sub>

**A14.8** page — `MainActivity.kt`, `sendPicked`

- Could not send that <sub>1035</sub>
- Sent ${sent.size}, then stopped <sub>1035</sub>

**A14.9** page — `MainActivity.kt`, `send`

- Could not send that <sub>1062</sub>

**A14.10** page — `MainActivity.kt`, `open`

- Open with <sub>1121</sub>
- Nothing on this phone can open that <sub>1123</sub>

**A14.11** page — `MainActivity.kt`, `saveCopy`

- Could not open the save dialog <sub>1138</sub>

**A14.12** page — `MainActivity.kt`, `saveAll`

- Could not open the folder picker <sub>1149</sub>

**A14.13** page — `MainActivity.kt`, `writeAll`

- could not create $name <sub>1167</sub>
- could not open $name <sub>1174</sub>
- Could not save them <sub>1184</sub>
- Saved $saved, then stopped <sub>1184</sub>

**A14.14** page — `MainActivity.kt`, `writeCopy`

- could not open the destination <sub>1204</sub>
- Could not save that file <sub>1212</sub>

**A14.15** page — `tabs.xml`, `tabs`

- Home <sub>6</sub>
- Files <sub>7</sub>
- Devices <sub>8</sub>
- Settings <sub>9</sub>

**A14.16** pop-up: “Transfers” — `MainActivity.kt`, `openTransfers`

- Could not read what is moving <sub>539</sub>
- Transfers <sub>542</sub>
- What you sent on purpose, and what was sent to you <sub>543</sub>
- Active <sub>548</sub>
- Syncing with your devices <sub>549</sub>
- Waiting to be collected <sub>551</sub>
- Nothing waiting. Files you send wait here until the device collects them. <sub>552</sub>
- Waiting for ${w.to}  ·  ${Words.size(w.size)} <sub>556</sub>
- Stop <sub>557</sub>
- Done <sub>565</sub>

**A14.17** pop-up: “Stop sending ${w.path.substringAfterLast('/')}?” — `MainActivity.kt`, `cancelSend`

- Stop sending ${w.path.substringAfterLast('/')}? <sub>583</sub>
- ${w.to} hasn't collected it yet, so it never arrives there. <sub>584</sub>
- Keep sending <sub>585</sub>
- Stop sending <sub>585</sub>
- Could not stop that <sub>595</sub>

**A14.18** pop-up: “Open your folder on ${ask.name}?” — `MainActivity.kt`, `askToOpen`

- Open your folder on ${ask.name}? <sub>770</sub>
- ${ask.name} is asking <sub>771</sub>
- ${ask.name} keeps your backups, your WhatsApp and your Private Vault, and  <sub>774</sub>
- shows them only after you say yes here. Approve, and it can <sub>775</sub>
- Your folder is kept on ${ask.name} sealed: it can't open it. Approve, and it can <sub>776</sub>
-  — for whoever is at that computer, until it's locked there, or ten minutes unused. ¶  ¶  <sub>777</sub>
- Only approve if you're there and asked for it. <sub>778</sub>
- Not now <sub>779</sub>
- Open it there <sub>779</sub>
- Confirm it's you <sub>790</sub>
- Open your folder on ${ask.name} <sub>790</sub>
- Could not answer ${ask.name} <sub>801</sub>

**A14.19** pop-up: “Add a device” — `MainActivity.kt`, `pair`

- Add a device <sub>826</sub>
- Your computer or another phone. It joins with a code, once. <sub>827</sub>
- Scan the other device's code <sub>829</sub>
- From a computer or phone that is showing one. <sub>830</sub>
- Show a code on this phone <sub>833</sub>
- Scan it with the other device, or type it there. <sub>834</sub>
- Enter a code <sub>835</sub>
- The six digits a device on this Wi-Fi is showing. <sub>836</sub>
- Visit someone's computer <sub>839</sub>
- As their guest. This phone keeps its own key. <sub>840</sub>

**A14.20** pop-up: “Enter a code” — `MainActivity.kt`, `typeCode`

- Enter a code <sub>868</sub>
- The six digits the other device shows <sub>868</sub>
- Both on the same Wi-Fi. Otherwise, scan its QR code. <sub>870</sub>
- Join <sub>871</sub>

**A14.21** pop-up — `MainActivity.kt`, `chooseDevice`

- Could not read your devices <sub>959</sub>
- No devices yet. Add one first. <sub>964</sub>
- Add a device <sub>965</sub>

**A14.22** pop-up: “Send” — `MainActivity.kt`, `chooseWhereToSend`

- Could not read your devices <sub>990</sub>
- Send <sub>993</sub>
- No devices yet. Add one first. <sub>995</sub>
- Add a device <sub>996</sub>
- My folder on ${peer.name} <sub>999</sub>
- Opens there only with a yes on this phone <sub>1000</sub>
- ${peer.name}'s Downloads <sub>1001</sub>
- Opens there freely · ${Words.seen(peer.lastSeen)} <sub>1002</sub>

**A14.23** pop-up: “Sent to ${to.name} before” — `MainActivity.kt`, `say`

- Sent to ${to.name} before <sub>1093</sub>
- ${nameOf(e.source)} — as ${e.sentAs}, ${Words.ago(e.at)} <sub>1094</sub>
- ${to.name} may still have it. Sending it again puts another copy there. <sub>1096</sub>
- ${to.name} may still have them. Sending them again puts another copy there. <sub>1097</sub>
- Send it again <sub>1100</sub>
- Send them again <sub>1100</sub>
- Don't send <sub>1101</sub>
- Leave it out <sub>1101</sub>
- Leave them out <sub>1101</sub>

**A14.24** message — `MainActivity.kt`, `confirmWithLock`

- Backing up WhatsApp. The first run starts on Wi-Fi. <sub>150</sub>
- That isn't WhatsApp's folder. Choose ${WhatsApp.relative().replace( <sub>152</sub>

**A14.25** message — `MainActivity.kt`, `heard`

- Your folder is open on ${answer.name}. It locks itself after ten minutes unused. <sub>301</sub>

**A14.26** message — `MainActivity.kt`, `cancelSend`

- Stopped <sub>591</sub>

**A14.27** message — `MainActivity.kt`, `sendGoes`

- Sending ${names.size} files to ${to.name}… <sub>733</sub>
- Sending ${names[0]} to ${to.name}… <sub>733</sub>
- Sending to ${to.name}. It carries on if you leave Qurb; a notification shows how far it has got. <sub>736</sub>

**A14.28** message — `MainActivity.kt`, `askToOpen`

- Set a screen lock on this phone first, so it can confirm it's you. <sub>787</sub>
- Opening your folder on ${ask.name}… <sub>798</sub>

**A14.29** message — `MainActivity.kt`, `joinWith`

- Looking for it on this Wi-Fi… <sub>896</sub>
- Connected to ${peer.name} <sub>913</sub>
- You visit ${peer.name} as a guest now <sub>913</sub>

**A14.30** message — `MainActivity.kt`, `addFiles`

- Added ${Words.files(added)}${if (addingPrivate)  <sub>944</sub>

**A14.31** message — `MainActivity.kt`, `writeAll`

- Saved ${Words.files(saved)} <sub>1182</sub>

**A14.32** message — `MainActivity.kt`, `writeCopy`

- Saved a copy <sub>1210</sub>

### A15. Sending into Qurb from another app

**A15.1** page — `ShareActivity.kt`, `onCreate`

- Qurb isn't set up on this phone <sub>51</sub>
- Open Qurb first and either create a key or enter your 24 words. <sub>52</sub>
- Nothing to save <sub>55</sub>
- That share did not contain a file Qurb can read. <sub>56</sub>

**A15.2** page — `ShareActivity.kt`, `chooseWhere`

- Qurb <sub>69</sub>
- ${uris.size} files <sub>78</sub>
- Save to Private Vault <sub>92</sub>
- Save to my folder on ${it.name} <sub>92</sub>
- Save to Files, on all your devices <sub>93</sub>
- Send to ${it.name} <sub>95</sub>
- Send to ${it.name}'s Downloads <sub>95</sub>
- Where should $what go? <sub>98</sub>
- Cancel <sub>106</sub>

**A15.3** page — `ShareActivity.kt`, `send`

- Sending to ${to.name} <sub>118</sub>
- Could not send that <sub>158</sub>
- Qurb could not read the file it was handed. <sub>158</sub>
- Sent $sent of ${sent + failed} <sub>164</sub>
- Sent to ${to.name} <sub>164</sub>
- ${to.name} collects  <sub>165</sub>
-  the next time it is switched on and reachable. Your other devices never see  <sub>166</sub>
- it. <sub>167</sub>
- them. <sub>167</sub>
-  ¶  ¶ $failed could not be read and were not sent. <sub>168</sub>

**A15.4** page — `ShareActivity.kt`, `askAgain`

- ${names[it.source] ?: it.source} — as ${it.sentAs}, ${Words.ago(it.at)} <sub>186</sub>

**A15.5** page — `ShareActivity.kt`, `say`

- Sent to ${to.name} before <sub>196</sub>
- ${to.name} may still have it. Sending it again puts another copy there. <sub>198</sub>
- ${to.name} may still have them. Sending them again puts another copy there. <sub>199</sub>
- Don't send <sub>202</sub>
- Leave it out <sub>202</sub>
- Leave them out <sub>202</sub>

**A15.6** page — `ShareActivity.kt`, `save`

- Private Vault <sub>242</sub>
- Qurb <sub>242</sub>
- Saving ${uris.size} files to $place <sub>243</sub>
- Saving to $place <sub>243</sub>
- Could not save that <sub>260</sub>
- Qurb could not read the file it was handed. <sub>260</sub>
- Saved $saved of ${saved + failed} <sub>274</sub>
- Saved to $place <sub>275</sub>
- Saved $saved files to $place <sub>276</sub>
- Your other devices will get $it the next time one is  <sub>295</sub>
- online. You don't need to do anything. <sub>296</sub>
- ${keepers[0].name} keeps a backup  <sub>298</sub>
- the next time it's online. Nobody else sees $it. <sub>299</sub>
- ${Words.list(keepers.map { k -> k.name })} keep a backup  <sub>300</sub>
- the next time each is online. Nobody else sees $it. <sub>301</sub>
- It stays <sub>303</sub>
- They stay <sub>303</sub>
-  on this phone and nowhere else. To keep a backup, choose a  <sub>304</sub>
- device in Qurb's Devices. <sub>305</sub>
-  ¶  ¶ $failed could not be read and was not saved. <sub>309</sub>

**A15.7** page — `activity_share.xml`, `activity_share`

- Saving to Qurb <sub>17</sub>
- Done <sub>41</sub>

**A15.8** message — `ShareActivity.kt`, `say`

- Send it again <sub>201</sub>
- Send them again <sub>201</sub>

### A16. Notifications

**A16.1** notification — `Notices.kt`, `asked`

- ${ask.name} asks to open your folder <sub>113</sub>
- Open Qurb to answer. Only approve if you're there and asked for it. <sub>114</sub>

**A16.2** notification — `Notices.kt`, `say`

- $it sent you a file <sub>134</sub>
- Somebody sent you a file <sub>134</sub>
- $it has $what <sub>136</sub>
- $what was collected <sub>136</sub>
- Delivered <sub>136</sub>
- Qurb could not finish something <sub>138</sub>

**A16.3** notification — `Notices.kt`, `channel`

- Sent, delivered, and what failed <sub>147</sub>
- A file somebody sent you, a file you sent arriving, and anything Qurb could not finish <sub>148</sub>

**A16.4** notification — `Transfers.kt`, `shown`

- Waiting for a device to collect ${Words.size(total)} <sub>41</sub>
- ${Words.size(sent)} of ${Words.size(total)} <sub>42</sub>

**A16.5** notification — `Transfers.kt`, `notice`

- Sending to your devices <sub>86</sub>

**A16.6** notification — `Transfers.kt`, `paused`

- Tap to finish sending <sub>125</sub>
- ${Words.size(waiting)} is waiting. A large file keeps sending in the background once it starts with Qurb open. <sub>126</sub>

**A16.7** notification — `Transfers.kt`, `channel`

- Sending to your devices <sub>152</sub>
- Shown while a large file is being collected from this phone <sub>153</sub>

### A17. Background sync

**A17.1** page — `SyncWorker.kt`, `doWork`

- ${outcome.adopted} file${if (outcome.adopted == 1u)  <sub>139</sub>
- } arrived <sub>139</sub>
- up to date <sub>140</sub>
- no device answered <sub>141</sub>
- ran out of time <sub>145</sub>
- no paired devices <sub>146</sub>
- adopted=${outcome.adopted} timedOut=${outcome.timedOut} <sub>168</sub>
- failed: ${e.message ?:  <sub>210</sub>

**A17.2** page — `SyncWorker.kt`, `lastRun`

- Last background sync ${Words.ago(at / 1000)}: $result. <sub>327</sub>

**A17.3** page — `SyncWorker.kt`, `state`

- Not scheduled. <sub>425</sub>
- Every hour, and whenever another device has something:  <sub>429</sub>
- it wakes this phone. <sub>430</sub>
- About every 15 minutes, when Android allows. <sub>431</sub>
- Running now. <sub>433</sub>
- Waiting on its conditions. <sub>434</sub>
- Cancelled. <sub>435</sub>
- Stopped after a failure it cannot retry past. Open the app and sync once. <sub>437</sub>
- Last run finished cleanly. <sub>438</sub>
-  ¶  ¶ Retried ${info.runAttemptCount} times — the other device may be asleep. <sub>440</sub>

### A18. Android's Manage space screen

**A18.1** page — `ManageSpaceActivity.kt`, `load`

- Qurb's storage on this phone <sub>60</sub>
- Qurb is not set up on this phone, so it keeps nothing here worth keeping. <sub>63</sub>
- Could not read Qurb's files: ${e.message} <sub>73</sub>
- Qurb uses ${Words.size(usage.onDisk)} here. <sub>78</sub>
- Every file here is also on another of your devices. Deleting Qurb's data loses  <sub>80</sub>
- none of them, though this phone would have to be set up again and joined to  <sub>81</sub>
- your other devices with a code. <sub>82</sub>
- ${Words.files(only.size)} (${Words.size(total)}) $verb only on this phone.  <sub>86</sub>
- Deleting Qurb's data deletes ${if (only.size == 1)  <sub>87</sub>
- } for good: <sub>87</sub>
- and ${only.size - 20} more <sub>90</sub>
- Open Qurb and sync with your computer first, and they will be safe there. <sub>91</sub>
- Free space safely <sub>94</sub>
- Freed ${Words.size(freed)} <sub>100</sub>
- Nothing to free <sub>100</sub>
- Only space nothing needs was freed: no file was removed. <sub>101</sub>
- ${Words.size(sent)} of that is this phone's copies of files it sent, kept after  <sub>108</sub>
- they arrived. <sub>109</sub>
- Let go of copies of sent files <sub>110</sub>
- Let go of ${Words.size(sent)}? <sub>112</sub>
- Keep <sub>114</sub>
- Let go <sub>115</sub>

**A18.2** page — `ManageSpaceActivity.kt`, `deleteButton`

- Delete everything Qurb keeps here… <sub>137</sub>
- Nothing in your folders exists only on this phone. <sub>138</sub>
- ${Words.files(only.size)} that ${if (only.size == 1)  <sub>139</sub>
- } only on this phone  <sub>139</sub>
- will be gone for good. <sub>140</sub>
- $files This phone's copies of files it sent (${Words.size(sent)}) go too. <sub>143</sub>
- Delete everything Qurb keeps here? <sub>145</sub>
- $lost This phone's key and its pairings go too. Your other devices keep what they have. <sub>146</sub>
- Keep <sub>147</sub>
- Delete everything <sub>148</sub>

### A19. Qurb in Android's Files app

**A19.1** page — `QurbDocumentsProvider.kt`, `queryRoots`

- Qurb <sub>81</sub>
- Synced across your devices <sub>82</sub>

**A19.2** page — `QurbDocumentsProvider.kt`, `queryDocument`

- Qurb <sub>102</sub>
- $path is not in qurb <sub>118</sub>

**A19.3** page — `QurbDocumentsProvider.kt`, `openDocument`

- no context <sub>190</sub>
- $path is not on this phone to write to <sub>195</sub>
- $path is not in qurb <sub>207</sub>
- ${path.substringAfterLast('/')} is on no device this phone syncs with any more. <sub>210</sub>
- ${path.substringAfterLast('/')} is not on this phone, and no device that has  <sub>228</sub>
- it answered. It will download at the next sync. <sub>229</sub>

**A19.4** page — `QurbDocumentsProvider.kt`, `createDocument`

- no context <sub>252</sub>

**A19.5** page — `QurbDocumentsProvider.kt`, `onDisk`

- $path is outside qurb's folder <sub>313</sub>

**A19.6** page — `QurbDocumentsProvider.kt`, `addFile`

- Not on this phone — downloads when opened <sub>340</sub>
- On no device <sub>343</sub>

### A20. Error messages

**A20.1** page — `Errors.kt`, `readable`

- That pairing code was not accepted. ¶  ¶  <sub>23</sub>
- Codes last five minutes and work once. Show a new one on the other device. <sub>24</sub>
- That recovery phrase was not accepted. ¶  ¶  <sub>27</sub>
- It is 24 words in order, separated by spaces. The phrase carries a  <sub>28</sub>
- checksum, so a single mistyped or swapped word is caught here rather  <sub>29</sub>
- than producing a key that opens nothing. <sub>30</sub>
- The key could not be unlocked. ¶  ¶ $detail <sub>33</sub>
- This device is not set up yet. <sub>36</sub>
- That file is not here any more. <sub>39</sub>
- The store could not be read or written. ¶  ¶ $detail <sub>42</sub>
- This is the only copy — no other device has it yet, so it cannot be freed here.  <sub>46</sub>
- Choose a device to keep your files, or send it somewhere first. <sub>47</sub>

**A20.2** page — `Errors.kt`, `network`

- Could not reach the other device: it did not answer. ¶  ¶  <sub>64</sub>
- Both devices have to be awake and running at the same moment.  <sub>65</sub>
- If they are, a firewall is the usual cause — sync uses UDP, and  <sub>66</sub>
- a rule that allows ping will still drop it. ¶  ¶  <sub>67</sub>
- On Linux: sudo ufw allow from 192.168.0.0/16 to any proto udp <sub>68</sub>
- The other device refused the connection. ¶  ¶  <sub>71</sub>
- Check it is still running, and that the address in the pairing  <sub>72</sub>
- code is the one it actually has now. <sub>73</sub>
- Could not reach the rendezvous service. ¶  ¶  <sub>76</sub>
- Check the address under Rendezvous service, and that `qurb signal`  <sub>77</sub>
- is running on that machine. From a phone it must be the computer's  <sub>78</sub>
- address on your network, not localhost. <sub>79</sub>
- That address could not be looked up. ¶  ¶ $detail <sub>82</sub>
- Network problem. ¶  ¶ $detail <sub>84</sub>

### A21. Messages from the engine

**A21.1** message — `lib.rs`, `create_protected`

- {} is already set up <sub>828</sub>
- a vault appeared while we were creating one <sub>848</sub>

**A21.2** message — `lib.rs`, `join_new`

- {root} is already set up <sub>907</sub>
- no runtime: {e} <sub>923</sub>
- that code has expired; ask the other device for a new one <sub>936</sub>
- the other device did not give its key: the code may already have been used, or that device needs updating <sub>939</sub>
- the other device said no <sub>942</sub>

**A21.3** message — `lib.rs`, `pairing_number`

- no runtime: {e} <sub>976</sub>

**A21.4** message — `lib.rs`, `protection_of`

- {root} has no vault <sub>1027</sub>

**A21.5** message — `lib.rs`, `export`

- {destination}: {e} <sub>1213</sub>

**A21.6** message — `lib.rs`, `visit_computer`

- that code adds one of your own devices, not a visit as a guest <sub>1343</sub>

**A21.7** message — `lib.rs`, `open_kept_folder`

- no such device <sub>1598</sub>
- {name} did not answer in time <sub>1627</sub>
- {name} isn't reachable: it has to be on, with Qurb running <sub>1664</sub>
- {name} isn't reachable: {e} <sub>1665</sub>

**A21.8** message — `lib.rs`, `share_targets`

- This phone <sub>1735</sub>

**A21.9** message — `lib.rs`, `set_sharing`

- not a device: {hex} <sub>1764</sub>

**A21.10** message — `lib.rs`, `conflicts`

- this phone <sub>1782</sub>
- another device <sub>1789</sub>

**A21.11** message — `lib.rs`, `settle_conflict`

- keep this, other or both, not {keep} <sub>1809</sub>
- other version <sub>1821</sub>

**A21.12** message — `lib.rs`, `recently_deleted`

- this phone <sub>1841</sub>

**A21.13** message — `lib.rs`, `approve_open`

- that computer is not asking any more <sub>1939</sub>

**A21.14** message — `lib.rs`, `choose_folder`

- {folder} is not one of this phone's folders <sub>2089</sub>

**A21.15** message — `lib.rs`, `keep_folder`

- {path}: {why} <sub>2225</sub>

**A21.16** message — `lib.rs`, `import`

- {source}: {e} <sub>2289</sub>

**A21.17** message — `lib.rs`, `open_inner`

- {root} has no vault <sub>2327</sub>

**A21.18** message — `lib.rs`, `device_of`

- no paired device {fingerprint} <sub>2408</sub>

**A21.19** message — `lib.rs`, `runtime`

- the runtime is unusable <sub>2467</sub>
- no runtime: {e} <sub>2481</sub>

**A21.20** message — `lib.rs`, `trade_asks`

- no answer in time <sub>2872</sub>

**A21.21** message — `lib.rs`, `look`

- the index reader failed earlier <sub>3078</sub>

**A21.22** message — `lib.rs`, `engine`

- the engine is in an unknown state after an earlier failure <sub>3097</sub>

**A21.23** message — `lib.rs`, `engine_version`

- engine {} · protocol {} · index schema {} <sub>3446</sub>

**A21.24** message — `lib.rs`, `qr_code`

- cannot draw that code: {e} <sub>3459</sub>

**A21.25** message — `lib.rs`, `list`

- {} keeps no folder for this phone <sub>3632</sub>

**A21.26** message — `lib.rs`, `download`

- not a file of the folder <sub>3655</sub>
- nowhere to put it <sub>3657</sub>
- the folder is closed <sub>3662</sub>

**A21.27** message — `lib.rs`, `stream`

- not a file of the folder <sub>3683</sub>
- {} no longer keeps {} <sub>3688</sub>

**A21.28** message — `lib.rs`, `read_at`

- the computer did not answer in time <sub>3733</sub>

**A21.29** message — `lib.rs`, `free_path`

- {stem} ({n}){ext} <sub>3761</sub>

**A21.30** message — `lib.rs`, `wait`

- pairing already failed <sub>3825</sub>
- this invitation has already been used <sub>3828</sub>
- pairing was cancelled <sub>3867</sub>

**A21.31** message — `lib.rs`, `a_long_pass_waits_a_minute_for_a_collector_to_come`

- ten seconds would go past the window <sub>3964</sub>
- never past what it may run for <sub>3966</sub>

**A21.32** message — `lib.rs`, `a_chosen_folder_is_one_of_the_phones_own`

- {good} refused <sub>3995</sub>
- {bad} accepted <sub>3998</sub>

**A21.33** message — `lib.rs`, `a_device_collecting_is_served_whoever_dialled`

- dialled in, collecting <sub>4014</sub>
- reached, something waiting <sub>4015</sub>
- reached, nothing waiting <sub>4016</sub>
- reached nobody, nobody collecting <sub>4017</sub>

**A21.34** message — `lib.rs`, `a_chunk_just_served_counts_as_collecting`

- nothing has gone yet <sub>4023</sub>

**A21.35** message — `lib.rs`, `the_screens_are_answered_while_the_engine_is_held`

- the screens waited {:?} <sub>4059</sub>

### A22. Shared words: sizes, times, counts

**A22.1** page — `Words.kt`, `ago`

- just now <sub>44</sub>

**A22.2** page — `Words.kt`, `plural`

- $n $unit${if (n == 1L)  <sub>52</sub>
- } ago <sub>52</sub>

**A22.3** page — `Words.kt`, `files`

- ${count(n.toLong())} files <sub>58</sub>
- 1 file <sub>58</sub>

**A22.4** page — `Words.kt`, `folders`

- $n folders <sub>59</sub>
- 1 folder <sub>59</sub>

**A22.5** page — `Words.kt`, `devices`

- $n devices <sub>61</sub>
- 1 device <sub>61</sub>

**A22.6** page — `Words.kt`, `seen`

- Last seen ${ago(it)} <sub>64</sub>
- Not connected yet <sub>64</sub>

**A22.7** page — `Words.kt`, `happened`

- another device <sub>73</sub>
- Saved on this phone <sub>75</sub>
- Deleted <sub>76</sub>
- Ready for $who <sub>79</sub>
- Collected by $who <sub>80</sub>
- Local space freed <sub>81</sub>
- Restored <sub>82</sub>
- Changed here and on $who <sub>83</sub>
- Paired with this phone <sub>86</sub>
- Removed from this phone <sub>87</sub>
- Send cancelled <sub>88</sub>
- Didn't finish <sub>89</sub>

**A22.8** page — `Words.kt`, `finished`

- another device <sub>100</sub>
- Sent to $who <sub>103</sub>
- Taken back before $who collected it <sub>104</sub>
- Didn't finish <sub>105</sub>

### A23. Shared components

**A23.1** page — `Kit.kt`, `of`

- Downloading <sub>379</sub>
- On $keptOn <sub>381</sub>
- On this phone <sub>382</sub>
- Only copy here <sub>383</sub>
- On no device <sub>385</sub>
- Available elsewhere <sub>386</sub>

**A23.2** page — `Kit.kt`, `kind`

- Image <sub>408</sub>
- Video <sub>409</sub>
- Audio <sub>410</sub>
- Archive <sub>411</sub>
- Code <sub>412</sub>
- Document <sub>413</sub>
- File <sub>414</sub>

**A23.3** page — `Kit.kt`, `buttons`

- Cancel <sub>540</sub>

### A24. Behind the scenes

**A24.1** page — `AndroidKeyStore.kt`, `put`

- could not write to the keystore: ${e.message} <sub>59</sub>

**A24.2** page — `AndroidKeyStore.kt`, `get`

- the stored key could not be decrypted: ${e.message} <sub>80</sub>

**A24.3** page — `Previews.kt`, `view`

- This version <sub>71</sub>

**A24.4** page — `Qurb.kt`, `discardUnfinished`

- This phone already holds files, so its key stays. <sub>99</sub>

**A24.5** page — `Qurb.kt`, `importUri`

- could not read that file <sub>248</sub>

**A24.6** page — `Qurb.kt`, `freeName`

- $stem ($n)$dot$extension <sub>310</sub>

**A24.7** page — `Qurb.kt`, `stage`

- could not read that file <sub>485</sub>

**A24.8** page — `SentCopies.kt`, `(top level)`

- These are this phone's copies of files it sent, which the devices you sent them to  <sub>10</sub>
- have taken. If one of those devices has deleted its copy since, this is the last one, and  <sub>11</sub>
- letting go of it loses that file. Files in your folders are not touched. <sub>12</sub>

**A24.9** page — `StreamProvider.kt`, `openFile`

- no longer open <sub>43</sub>
- read only <sub>44</sub>

## The desktop window

### D1. Setup

**D1.1** page — `index.html`, `(top level)`

- width=device-width, initial-scale=1 <sub>5</sub>
- Qurb <sub>6</sub>
- onboarding hidden <sub>16</sub>
- sheet glass-elevated <sub>17</sub>
- step welcome on <sub>18</sub>
- Your devices. <sub>21</sub>
- Your files. <sub>21</sub>
- Your space. <sub>21</sub>
- Get started <sub>23</sub>
- btn primary large <sub>23</sub>
- Is this your first device? <sub>28</sub>
- Qurb keeps your files on devices you own, and moves them directly between them. <sub>29</sub>
- Set up Qurb here <sub>33</sub>
- This computer becomes your first device. <sub>33</sub>
- Add this computer with the code your other device shows. <sub>38</sub>
- I already use Qurb <sub>38</sub>
- Back <sub>42</sub>
- actions end <sub>42</sub>
- Where should your files live? <sub>46</sub>
- A folder on this computer. Everything in it becomes part of your Qurb space. <sub>47</sub>
- Folder <sub>49</sub>
- Continue <sub>55</sub>
- btn primary <sub>55</sub>
- How much space may Qurb use here? <sub>60</sub>
- On this computer's disk. You can change it at any time in Storage. <sub>61</sub>
- Another amount <sub>65</sub>
- In GB — 75, or 1.5 TB <sub>67</sub>
- Enter the code from your other device <sub>81</sub>
- Add a device <sub>82</sub>
- On your phone, open Qurb, go to Devices, choose <sub>82</sub>
- Show a code on this phone <sub>83</sub>
- Code <sub>86</sub>
- The six digits, or the whole code <sub>88</sub>
- field hidden <sub>90</sub>
- Recovery phrase <sub>91</sub>
- wheel push industry … <sub>93</sub>
- says warn hidden <sub>95</sub>
- Use my 24 words instead <sub>98</sub>
- btn ghost <sub>98</sub>
- Add this computer <sub>99</sub>
- mark away <sub>106</sub>
- Qurb is locked <sub>107</sub>
- margin-top: 24px <sub>107</sub>
- This computer's key is protected by a passphrase. Type it to open your files and start syncing. <sub>108</sub>
- Passphrase <sub>110</sub>
- Unlock <sub>115</sub>
- .qurb <sub>118</sub>
- You're set up <sub>124</sub>
- Open Qurb <sub>129</sub>

**D1.2** page — `setup.js`, `refuse`

- Qurb is already set up in this folder. Choose another, or open that one instead. <sub>89</sub>
- Qurb is already set up in this folder, with its own key. <sub>90</sub>
- This folder can't be written to. <sub>91</sub>
-  · ${size(folder.free)} free on this disk <sub>93</sub>
-  at ${folder.path} <sub>97</sub>
- A new folder${at}${disk} <sub>99</sub>
- Over ${folder.existing_files} <sub>105</sub>
- ${n} things already here${at} — all of them will sync${disk} <sub>106</sub>

**D1.3** page — `setup.js`, `gb`

- ${(n / 1024).toFixed(1)} TB <sub>132</sub>
- ${n < 10 ? n.toFixed(1) : n.toFixed(0)} GB <sub>133</sub>

**D1.4** page — `setup.js`, `pick`

- ${amount} GB <sub>153</sub>
- More than the ${gb(free)} free on this disk <sub>158</sub>
- Another amount <sub>168</sub>

**D1.5** page — `setup.js`, `tellAboutAllowance`

- ${gb(free)} free on this disk. <sub>186</sub>
-  Larger amounts are more than it has free. <sub>188</sub>

**D1.6** page — `setup.js`, `readCustom`

- ${gb(bytes)} is more than the ${gb(free)} free on this disk. <sub>208</sub>
- ${gb(bytes)}, of ${gb(free)} free. <sub>213</sub>
- This computer is your first device, and Qurb is watching your folder. Add your phone from Devices, with a code. <sub>233</sub>
- Enter the code from your other device <sub>250</sub>
- Enter your recovery phrase <sub>250</sub>
- The 24 words of your key. Order matters; spacing and capitals don't. <sub>252</sub>
- On your phone, open Qurb, go to Devices, choose Add a device and then Show a code on this phone. Type its six digits here, on the same Wi-Fi. It brings your key with it, so there's nothing else to type. <sub>253</sub>
- Use a code instead <sub>254</sub>
- Use my 24 words instead <sub>254</sub>
- This computer now shares your key, and Qurb is watching your folder. Pair it with your other devices from Devices. <sub>268</sub>
- Looking for it on this network… <sub>274</sub>
- Approve this computer on your phone. It should show ${number}. <sub>276</sub>
- This computer is now one of your devices, paired with ${joined}, and Qurb is watching your folder. <sub>279</sub>

### D2. The window: sidebar and frame

**D2.1** page — `app.js`, `decide`

- Qurb can't open this folder <sub>52</sub>

**D2.2** page — `index.html`, `(top level)`

- app hidden <sub>135</sub>
- sidebar glass-clear <sub>137</sub>
- Qurb <sub>140</sub>
- Home <sub>142</sub>
- nav on <sub>142</sub>
- Files <sub>143</sub>
- Devices <sub>144</sub>
- Storage <sub>145</sub>
- Private <sub>146</sub>
- Private Vault <sub>147</sub>
- Settings <sub>149</sub>
- stage glass-frosted <sub>152</sub>
- btn small transfers-chip hidden <sub>155</sub>
- i small <sub>156</sub>
- transfers <sub>157</sub>
- position: absolute <sub>487</sub>

### D3. Home

**D3.1** page — `home.js`, `heroFor`

- Add your first device <sub>20</sub>
- Qurb keeps your files on devices you own. Connect your phone or another computer to begin. <sub>21</sub>
- Add a device <sub>22</sub>
- Send <sub>25</sub>
- up to date <sub>27</sub>
- Everything is synced. <sub>30</sub>
- Your files are safe. ${conflicts === 1 ? "One thing needs" :  <sub>32</sub>
- } your attention. <sub>32</sub>
- Your files are safe. Nothing needs your attention. <sub>33</sub>
- Syncing… <sub>40</sub>
- Receiving ${count(n, "file")} from ${s.incoming[0].device}. <sub>42</sub>
- Bringing your devices up to date. <sub>43</sub>
- no devices reachable <sub>47</sub>
-  ${count(st.only_here, "file")} ${st.only_here === 1 ? "is" : "are"} only on this computer until then. <sub>49</sub>
- Your devices are away. <sub>53</sub>
- Changes here reach them the next time one is online.${alone} <sub>54</sub>
- needs attention <sub>58</sub>
- Something needs attention. <sub>61</sub>
- Qurb couldn't finish something. Activity, at the clock, says what. <sub>62</sub>
- Getting ready… <sub>68</sub>
- Catching up with your folder. <sub>69</sub>

**D3.2** page — `home.js`, `drawHome`

- Qurb can't read its state <sub>85</sub>
- up to date <sub>101</sub>
- ${size(st.used)} used <sub>105</sub>
- ${count(s.peers, "device")} connected <sub>108</sub>
- ${s.peers_reachable} of ${count(s.peers, "device")} connected <sub>109</sub>

**D3.3** page — `home.js`, `setHero`

- mark ${mark ?? ""} <sub>116</sub>

**D3.4** page — `home.js`, `drawWaiting`

- Waiting <sub>205</sub>
- Not on ${going} yet <sub>210</sub>
- more file <sub>215</sub>
- Sent to ${o.to}, not collected yet <sub>218</sub>
- Not collected yet <sub>223</sub>
- more send <sub>223</sub>
- ${d.name}'s folders <sub>234</sub>
- ${count(files - kept, "file")} still to come from it <sub>235</sub>
- Everything here has reached ${going}. <sub>241</sub>
- Nothing waiting <sub>241</sub>

**D3.5** page — `home.js`, `drawBackups`

- Backups <sub>253</sub>
- group glass-frosted <sub>254</sub>

**D3.6** page — `home.js`, `item`

- WhatsApp, from ${d.name} <sub>265</sub>
- ${count(d.whatsapp.files, "file")}, ${size(d.whatsapp.bytes)}, kept here in its Private Vault, out of sight <sub>266</sub>
- Off. Turned on in Qurb on ${d.name}, under Backups <sub>267</sub>
- Change… <sub>270</sub>
- Choose… <sub>270</sub>
- btn small <sub>270</sub>
- Folders, from ${d.name} <sub>272</sub>
- ${d.name} hasn't listed its folders here yet <sub>273</sub>
- None chosen yet: photos, documents, any folder of the phone <sub>274</sub>
- ${count(n, "folder")}: ${kept >= files ?  <sub>275</sub>
- } kept here <sub>275</sub>

**D3.7** page — `home.js`, `drawAttention`

- ${base(found[0].path)} has two versions. <sub>297</sub>
- ${found.length} files have two versions. <sub>298</sub>
- Review <sub>300</sub>
- btn small <sub>300</sub>
- ${found.length === 1 ? "1 thing needs" :  <sub>304</sub>
- } attention <sub>304</sub>
- Both versions are kept. Choose which you want. <sub>305</sub>

**D3.8** page — `home.js`, `preview`

- ${base(path)}, this version <sub>338</sub>

**D3.9** page — `home.js`, `conflictCard`

- card glass-frosted <sub>351</sub>
- display:grid;place-items:center;width:40px;height:40px;border-radius:11px;background:rgba(var(--paper),.8);border:1px solid var(--hairline);color:var(--text-2) <sub>356</sub>
- Qurb <sub>359</sub>
- display:grid;grid-template-columns:1fr 1fr;gap:12px;margin-top:16px <sub>364</sub>

**D3.10** page — `home.js`, `side`

- padding:14px;border-radius:14px;background:rgba(var(--paper),.66);border:1px solid var(--hairline) <sub>367</sub>
- Since deleted or renamed <sub>370</sub>
- quiet num <sub>377</sub>
- state elsewhere <sub>380</sub>
- Not on this computer yet <sub>382</sub>
- The other version <sub>387</sub>
- This version <sub>387</sub>

**D3.11** page — `home.js`, `choose`

- btn small <sub>392</sub>
- Its contents aren't on this computer yet <sub>394</sub>
- Kept ${base(kept)} <sub>406</sub>
- Kept both <sub>406</sub>
- The other version is now ${base(kept)}. <sub>408</sub>
- The other version is in Recently deleted for 30 days. <sub>409</sub>
- says warn <sub>414</sub>
- Keep this version <sub>420</sub>
- Keep the other version <sub>421</sub>
- Keep both <sub>422</sub>
- Saved on this computer <sub>431</sub>
- Arrived <sub>432</sub>
- Deleted <sub>433</sub>
- Ready to send <sub>434</sub>
- Collected <sub>435</sub>
- Local space freed <sub>436</sub>
- Restored <sub>437</sub>
- Changed on two devices <sub>438</sub>
- Paired with this computer <sub>439</sub>
- Didn't finish <sub>440</sub>
- Send cancelled <sub>441</sub>
- Removed from this computer <sub>442</sub>

**D3.12** page — `home.js`, `moving`

-  · about ${duration((total - done) / rate)} left <sub>558</sub>

**D3.13** page — `home.js`, `drawTransfers`

- Receiving from ${t.device} <sub>580</sub>
- Sending to ${t.device} <sub>581</sub>
- Active <sub>583</sub>
- No active transfers <sub>585</sub>
- Waiting to be collected <sub>596</sub>
- Cancel <sub>600</sub>
- Stop sending? <sub>600</sub>
- btn small reveal <sub>600</sub>
- Waiting for ${o.to} <sub>614</sub>

**D3.14** page — `home.js`, `finishedLine`

- Sent to ${who} <sub>632</sub>
- Taken back before ${who} collected it <sub>634</sub>
- Didn't finish <sub>636</sub>

**D3.15** page — `home.js`, `drawFinished`

- Done <sub>648</sub>
- ; saved to  <sub>659</sub>
- Show <sub>660</sub>
- btn small reveal <sub>660</sub>
- Nothing yet <sub>677</sub>
- says warn <sub>679</sub>

**D3.16** page — `index.html`, `(top level)`

- screen on <sub>166</sub>
- Getting ready… <sub>170</sub>
- Activity <sub>173</sub>
- icon-btn hero-aside <sub>173</sub>
- btn large <sub>176</sub>
- Sync now <sub>177</sub>
- btn primary large <sub>179</sub>
- Send <sub>180</sub>
- secondary-facts num <sub>183</sub>

**D3.17** pop-up — `home.js`, `openConflicts`

- ${found.length} files with two versions <sub>322</sub>
- Two versions <sub>322</sub>
- Two devices changed these at the same time, so Qurb kept both. Nothing is lost whichever you choose. <sub>323</sub>
- Nothing to review. Every file has one version. <sub>325</sub>

**D3.18** pop-up — `home.js`, `transfersOpen`

- Transfers <sub>539</sub>
- What you sent on purpose, and what was sent to you. <sub>539</sub>

**D3.19** message — `home.js`, `settle`

- Syncing with your devices <sub>151</sub>

**D3.20** message — `home.js`, `drawTransfers`

- Stopped sending ${base(o.path)} <sub>603</sub>

### D4. Activity

**D4.1** page — `home.js`, `happenedRow`

- ${words} from ${other} <sub>454</sub>
- ${words} by ${other} <sub>455</sub>
- Ready for ${other} <sub>456</sub>
- Changed here and on ${other} <sub>457</sub>

**D4.2** page — `home.js`, `drawActivity`

- Nothing has happened yet. <sub>481</sub>

**D4.3** page — `index.html`, `(top level)`

- Home <sub>464</sub>
- i small <sub>464</sub>
- margin-top: 0 <sub>465</sub>
- Activity <sub>466</sub>
- Everything this computer did, newest first. <sub>466</sub>
- Show older <sub>469</sub>
- btn hidden <sub>469</sub>

### D5. Files

**D5.1** page — `files.js`, `makeBrowser`

- ${count(found.length, "result")} for “${b.searching}” <sub>36</sub>
- Nothing matches that name. <sub>40</sub>
- Folders <sub>48</sub>
- Files <sub>51</sub>
- This folder is empty. <sub>56</sub>
- Nothing here yet. Turn on Keep new files private in Settings, and files you add on this computer come here. <sub>57</sub>
- Your Qurb folder is empty. Put files in it, and they appear on your other devices. <sub>59</sub>

**D5.2** page — `files.js`, `go`

- Folder <sub>80</sub>
- Shared <sub>80</sub>

**D5.3** page — `files.js`, `folderTile`

- Folder options <sub>103</sub>

**D5.4** page — `files.js`, `drawTop`

- Folders <sub>114</sub>

**D5.5** page — `files.js`, `tile`

- folder area <sub>116</sub>
- On all your devices <sub>126</sub>
- Shared <sub>126</sub>
- Nothing kept yet <sub>130</sub>

**D5.6** page — `files.js`, `drawPerson`

- Folder <sub>148</sub>
- Open on this computer. It locks itself after ten minutes unused. <sub>173</sub>
- Lock <sub>174</sub>
- btn small <sub>174</sub>
- Folders <sub>182</sub>
- Phones <sub>182</sub>
- Files <sub>189</sub>
- The first 1,000 of ${count(answer.files.length, "file")}. <sub>204</sub>
- Nothing is kept here yet. <sub>206</sub>
- This folder is empty. <sub>206</sub>

**D5.7** page — `files.js`, `lockedFolder`

- empty locked-folder <sub>213</sub>
- Waiting for your phone. Open Qurb on it and approve: it asks as soon as it next syncs with this computer. <sub>219</sub>
- Waiting for ${p.name}. They approve on their phone, which asks when it next syncs with this computer. <sub>220</sub>
- Your folder opens with a yes on your phone: its backups, its WhatsApp and its Private Vault, as this computer keeps them. <sub>224</sub>
- ${p.name}'s folder is sealed. It opens only when they approve on their phone. <sub>225</sub>
- Ask ${p.name} <sub>226</sub>
- Ask your phone <sub>226</sub>
- btn primary <sub>226</sub>
- This computer holds the same key as your phone, so this is Qurb's rule rather than a lock on the files themselves. <sub>238</sub>
- Qurb <sub>258</sub>
- Private Vault <sub>262</sub>

**D5.8** page — `files.js`, `liveState`

- state moving <sub>270</sub>
- Downloading <sub>271</sub>

**D5.9** page — `files.js`, `fileRow`

- icon-btn reveal <sub>278</sub>
- Keep here <sub>290</sub>
- btn small reveal <sub>290</sub>
- Qurb <sub>302</sub>

**D5.10** page — `files.js`, `isHere`

- only here <sub>317</sub>

**D5.11** page — `files.js`, `fileActions`

- Open <sub>326</sub>
- Show in folder <sub>327</sub>
- Keep on this device <sub>329</sub>
- Free local space <sub>332</sub>
- Send to device… <sub>334</sub>
- Move to shared <sub>337</sub>
- Move to Private Vault… <sub>338</sub>
- Details <sub>340</sub>
- Delete <sub>342</sub>

**D5.12** page — `files.js`, `onlyCopy`

- This is the only copy currently stored in Qurb, so it can't be freed. <sub>393</sub>

**D5.13** page — `files.js`, `redraw`

- Keep on this device <sub>522</sub>
- btn primary <sub>522</sub>
- Open <sub>524</sub>
- Show in folder <sub>525</sub>
- Free local space <sub>528</sub>
- Send to device… <sub>530</sub>
- Move to shared <sub>532</sub>
- Move to Private Vault… <sub>533</sub>
- Delete <sub>535</sub>
- btn ghost quiet-danger <sub>535</sub>
- History <sub>540</sub>

**D5.14** page — `files.js`, `drawWhat`

- Drop files or folders here <sub>769</sub>
- ${names.slice(0, 2).join(", ")} and ${names.length - 2} more <sub>772</sub>
- Choose files… <sub>776</sub>
- btn small <sub>776</sub>
- Choose a folder… <sub>777</sub>

**D5.15** page — `files.js`, `pickFromDisk`

- No file chooser here — drag them in instead. <sub>790</sub>

**D5.16** page — `files.js`, `drawTo`

- says warn <sub>805</sub>
- Add a device <sub>809</sub>
- btn small <sub>809</sub>
- No devices yet. <sub>811</sub>
- Connected now <sub>822</sub>
- Last seen ${when(d.last_seen)} <sub>822</sub>
- Not connected yet <sub>822</sub>

**D5.17** page — `files.js`, `send`

- Getting ready to send to ${d.name}… <sub>848</sub>

**D5.18** page — `files.js`, `askAgain`

- Sent to ${d.name} before <sub>866</sub>
- as ${e.sent_as} <sub>869</sub>
- ${d.name} may still have it. Sending it again puts another copy there. <sub>873</sub>
- ${d.name} may still have them. Sending them again puts another copy there. <sub>874</sub>
- Send it again <sub>876</sub>
- Send them again <sub>876</sub>
- btn primary <sub>876</sub>
- Don't send <sub>877</sub>
- Leave it out <sub>877</sub>
- Leave them out <sub>877</sub>

**D5.19** page — `files.js`, `end`

- end ${cls} <sub>894</sub>
- This computer <sub>900</sub>
- Nothing was sent. <sub>904</sub>

**D5.20** page — `files.js`, `watch`

- journey landed <sub>917</sub>
- Sent. ${d.name} has ${what1}. <sub>918</sub>
- journey moving <sub>921</sub>
- journey waiting <sub>921</sub>
- Sending ${what1} to ${d.name}… <sub>923</sub>
- ${what1[0].toUpperCase()}${what1.slice(1)} ${r.sent === 1 ? "is" : "are"} ready for ${d.name}. It collects ${r.sent === 1 ? "it" : "them"} the next time it's online — keep ${r.sent === 1 ? "it as it is" : "them as they are"} until then. You can close this. <sub>926</sub>
- ).join("; ")}. <sub>934</sub>
- Done <sub>937</sub>
- btn primary <sub>937</sub>

**D5.21** page — `index.html`, `(top level)`

- Files <sub>194</sub>
- Everything in your Qurb space, and where it is. <sub>195</sub>
- More <sub>197</sub>
- Search files <sub>202</sub>

**D5.22** pop-up: “Move ${name} to Private Vault?” — `files.js`, `move`

- Move ${name} to Private Vault? <sub>416</sub>
- It stays on this computer, and your other devices remove their copies at their next sync —  <sub>418</sub>
- each keeps it in Recently deleted for 30 days, as with any deletion. <sub>419</sub>
- ${keepers.join(", ")} keeps a backup of this computer's Private Vault, so it keeps this file too. <sub>421</sub>
- No device keeps a backup of this computer's Private Vault, so this computer will have the only copy. <sub>422</sub>
- actions end <sub>423</sub>
- Cancel <sub>424</sub>
- Move <sub>426</sub>
- btn primary <sub>426</sub>

**D5.23** pop-up: “Has another version” — `files.js`, `openDetails`

- ${f.private ? "Private Vault" : "Qurb"}${folderOf(f.path) ?  <sub>463</sub>
- only here <sub>468</sub>
- badge ${tone} <sub>469</sub>
- Downloading <sub>470</sub>
- badge neutral <sub>473</sub>
- Private <sub>475</sub>
- This is the only copy currently stored in Qurb. It reaches your other devices the next time one is online. <sub>480</sub>
- No device this computer syncs with has this file. It was freed here while another device kept it, and that device no longer has it or is no longer paired. It stays listed so you know it existed; deleting it removes it from your devices' lists. <sub>483</sub>
- This computer <sub>487</sub>
- Type <sub>492</sub>
- Size <sub>493</sub>
- Location <sub>494</sub>
- No device <sub>495</sub>
- No device has told this one yet <sub>495</sub>
- Modified <sub>496</sub>
- Review <sub>505</sub>
- btn small <sub>505</sub>
- Has another version <sub>507</sub>
- Two devices changed it at once. <sub>507</sub>

**D5.24** pop-up — `files.js`, `openFolderOptions`

- Which devices have this folder, and whether this computer keeps its files. <sub>663</sub>
- On these devices <sub>665</sub>
- group glass-frosted <sub>666</sub>
- A device you leave out keeps what it already has, and gets nothing new. <sub>681</sub>
- says warn hidden <sub>682</sub>
- Save <sub>683</sub>
- btn primary <sub>683</sub>
- On this computer <sub>700</sub>
- Downloaded when opened <sub>704</sub>
- Kept on this computer <sub>704</sub>
- Its files are listed here and download when you keep one. <sub>706</sub>
- Free local space to list its files here without keeping them. Nothing is deleted. <sub>707</sub>
- Free local space <sub>708</sub>
- Keep here <sub>708</sub>
- btn small <sub>708</sub>
-  ${count(r.kept.length, "file")} stayed: this computer has the only copy. <sub>717</sub>
- actions end <sub>729</sub>

**D5.25** pop-up — `files.js`, `openSend`

- Send to device <sub>751</sub>
- To one device, and nowhere else. It isn't added to your other devices. <sub>752</sub>
- actions end <sub>757</sub>
- Send <sub>758</sub>
- btn primary <sub>758</sub>

**D5.26** message — `files.js`, `keepHere`

- Downloading ${base(path)} to this computer <sub>367</sub>

**D5.27** message — `files.js`, `freeLocal`

- Freed ${size(freed)}. ${base(path)} stays in Qurb. <sub>383</sub>

**D5.28** message — `files.js`, `move`

- ${name} goes to all your devices. <sub>405</sub>
- ${name} is in Private Vault. <sub>405</sub>

**D5.29** message — `files.js`, `deleteFile`

- Deleted ${base(path)} from your devices. It's in Recently deleted for 30 days. <sub>437</sub>

**D5.30** message — `files.js`, `openFolderOptions`

- “${folder}” has no options yet: put a file in it first. <sub>659</sub>
- ${folder} is on all your devices <sub>691</sub>
- Saved who has ${folder} <sub>691</sub>
- ${count(asked, "file")} on the way back <sub>714</sub>
- ${folder} is kept here <sub>714</sub>
- Freed ${size(r.bytes)} from ${folder}.${kept} <sub>718</sub>

### D6. Private Vault

**D6.1** page — `index.html`, `(top level)`

- Private Vault <sub>215</sub>
- Private to this computer. Only this computer can see these files; a device you choose can keep a backup. <sub>216</sub>
- Search Private Vault <sub>222</sub>

### D7. Recently deleted

**D7.1** page — `files.js`, `drawDeleted`

- Nothing deleted recently. <sub>567</sub>
- Restore <sub>572</sub>
- btn small <sub>572</sub>
- Delete for good? <sub>573</sub>
- Delete now <sub>573</sub>
- btn small ghost quiet-danger reveal <sub>573</sub>
- ${d.private ? "Private Vault · " : ""}Deleted ${when(d.at)}${d.by ?  <sub>585</sub>
- Expires in ${count(days, "day")} <sub>586</sub>
- Expires today <sub>586</sub>
- Restored as ${base(at)}: something else is at ${base(d.path)} now. <sub>596</sub>
- ${base(at)} is back in Private Vault. <sub>598</sub>
- ${base(at)} is back, on all your devices. <sub>599</sub>

**D7.2** page — `files.js`, `drawDeletedLink`

- Recently deleted <sub>623</sub>
- ${count(entries.length, "file")} · kept for 30 days <sub>624</sub>
- Kept for 30 days <sub>624</sub>
- Options for “ <sub>638</sub>

**D7.3** page — `index.html`, `(top level)`

- Files <sub>473</sub>
- i small <sub>473</sub>
- margin-top: 0 <sub>474</sub>
- Recently deleted <sub>475</sub>
- Files deleted on any of your devices are kept for 30 days. Restoring one puts it back where it was. <sub>476</sub>

### D8. Devices

**D8.1** page — `devices.js`, `presence`

- Connected <sub>12</sub>
- Connected through the relay <sub>13</sub>
- Last seen ${when(d.last_seen)} <sub>14</sub>
- Not connected yet <sub>15</sub>

**D8.2** page — `devices.js`, `presenceLine`

- presence on <sub>20</sub>

**D8.3** page — `devices.js`, `drawDevices`

- device self glass-frosted <sub>36</sub>
- presence on <sub>40</sub>
- This computer <sub>41</sub>
- Guests <sub>50</sub>
- Computers you visit as a guest <sub>51</sub>

**D8.4** page — `devices.js`, `drawCard`

- device glass-frosted <sub>68</sub>
- ${count(waiting, "file")} waiting for it <sub>74</sub>
- Add a device <sub>82</sub>
- btn primary <sub>82</sub>
- Add your phone or another computer, and your files move between them. <sub>84</sub>

**D8.5** page — `devices.js`, `cover`

- Backed up with ${outer.input.value} <sub>297</sub>
- says warn hidden <sub>303</sub>
- ${input.value} is backed up from the next time ${d.name} syncs <sub>310</sub>
- ${input.value} backs up nothing new <sub>311</sub>
- Unticked, a folder backs up nothing new. What is kept here already stays. <sub>321</sub>

**D8.6** page — `devices.js`, `draw`

- Kept here sealed: this computer can't open it. To open it here, ask ${d.name}. <sub>404</sub>
- Ask ${d.name}'s phone <sub>405</sub>
- btn primary <sub>405</sub>
- Waiting for ${d.name}. Open Qurb on their phone and approve — it asks for their fingerprint, face or screen lock. <sub>414</sub>
- Stop asking <sub>415</sub>
- Open here. Anyone at this computer can see these files until you lock it. It locks itself after ten minutes unused. <sub>423</sub>
- Nothing in it. <sub>435</sub>
- Lock <sub>437</sub>

**D8.7** page — `devices.js`, `askToRemove`

- confirm card glass-frosted <sub>464</sub>
- Remove ${called ?? plan.name}? <sub>468</sub>

**D8.8** page — `devices.js`, `say`

- its key and everything already on it — removing it deletes nothing there. <sub>476</sub>
- ${plan.name}. Once it's removed, ${n === 1 ? "it" : "they"} can't be downloaded again. <sub>482</sub>
- Keep them here first <sub>483</sub>
- btn small <sub>483</sub>
- Asked for. They come back the next time it's reachable; remove it after that. <sub>488</sub>
- Also delete the ${count(plan.kept, "file")} (${size(plan.kept_bytes)}) this computer keeps for it <sub>498</sub>
- actions end <sub>503</sub>
- Cancel <sub>504</sub>
- Remove device <sub>505</sub>
- btn danger <sub>505</sub>

**D8.9** page — `devices.js`, `choose`

- Add a device <sub>555</sub>
- Your phone or another computer. It joins with a code, once. <sub>555</sub>

**D8.10** page — `devices.js`, `option`

- Scan it with your phone, or type its six digits there. <sub>568</sub>
- Show a code <sub>568</sub>
- Enter a code <sub>569</sub>
- The six digits a device on this network is showing. <sub>569</sub>
- Add a person <sub>571</sub>
- Someone else's phone, as a guest. It keeps its own key and sees only what you send it. <sub>571</sub>

**D8.11** page — `devices.js`, `show`

- Scan or type this on the other device <sub>576</sub>
- Scan or type this on their phone <sub>576</sub>
- In Qurb on their phone: Devices → Add → Visit a computer. <sub>577</sub>
- Opening a port… <sub>578</sub>
- Couldn't show a code <sub>584</sub>
- says warn <sub>584</sub>
- Back <sub>585</sub>
- actions end <sub>587</sub>
- meta digits-say <sub>607</sub>
- Or type these on their phone, on this Wi-Fi <sub>608</sub>
- Or type these on your phone, on this Wi-Fi <sub>609</sub>
- digits num <sub>610</sub>
- Type the whole code or read it out instead <sub>616</sub>
- On another computer: <sub>617</sub>
- On their computer: <sub>617</sub>
- qurb join <folder> ${invitation.code} <sub>618</sub>
- qurb visit ${invitation.code} <sub>618</sub>
- Or read this out: <sub>620</sub>
- The code carries this computer's identity, which is why it travels outside the network; the six digits fetch it, for whoever knows them. It works once. <sub>626</sub>
- Stop showing it <sub>628</sub>
- asking glass-frosted hidden <sub>640</sub>
- margin-top: 16px; padding: 16px; border-radius: 16px <sub>642</sub>

**D8.12** page — `devices.js`, `ask`

- wants to visit this computer as a guest, with their own key, seeing only what you send them <sub>650</sub>
- wants to join and take this computer's key <sub>651</sub>
- wants to pair <sub>651</sub>
- font-size: 32px; font-weight: 600; letter-spacing: 2px; margin: 8px 0; font-variant-numeric: tabular-nums <sub>654</sub>
- Decline <sub>655</sub>
- Approve <sub>656</sub>
- btn primary <sub>656</sub>

**D8.13** page — `devices.js`, `answer`

- actions end <sub>666</sub>
- Approve only if it shows this number: <sub>670</sub>
- A different number means someone else has this code: decline. <sub>672</sub>

**D8.14** page — `devices.js`, `follow`

- A device is asking to join. <sub>688</sub>
- Someone is asking to visit. <sub>688</sub>
- These were typed wrongly too often. Scan the code instead. <sub>697</sub>
- Waiting — this code works for ${Math.floor(left / 60)}:${String(left % 60).padStart(2, "0")} <sub>703</sub>
- That code has expired. Show a new one. <sub>709</sub>
- Pairing didn't work. <sub>710</sub>

**D8.15** page — `devices.js`, `enter`

- Enter a code <sub>717</sub>
- The six digits the other device is showing, on the same network. The whole code works too. <sub>717</sub>
- input digits-input num <sub>718</sub>
- says warn hidden <sub>725</sub>
- actions end <sub>726</sub>
- Back <sub>727</sub>
- Join <sub>729</sub>
- btn primary <sub>729</sub>

**D8.16** page — `devices.js`, `join`

- Looking for it on this network… <sub>738</sub>
- Approve it on the other device. It should show ${number}. <sub>743</sub>

**D8.17** page — `devices.js`, `done`

- device glass-frosted arrived <sub>769</sub>
- margin: 8px auto 0; max-width: 280px; align-items: center; text-align: center <sub>771</sub>
- Connected to your Qurb space <sub>778</sub>
- presence on <sub>778</sub>
- Device added <sub>780</sub>
- actions end <sub>781</sub>
- Done <sub>782</sub>
- btn primary <sub>782</sub>

**D8.18** page — `index.html`, `(top level)`

- Devices <sub>233</sub>
- Your devices, and whether each is reachable. <sub>234</sub>
- Add a device <sub>236</sub>
- btn primary <sub>236</sub>

**D8.19** pop-up — `devices.js`, `openDevice`

- A guest: another person's device, with its own key. It sees only what you send it. <sub>127</sub>
- Another person's computer, which you visit as a guest. It sees only what you send it. <sub>128</sub>
- Status <sub>131</sub>
- Paired <sub>132</sub>
- Who <sub>133</sub>
- Their folder <sub>137</sub>
- ${size(d.kept)} kept here, sealed: you cannot open it <sub>138</sub>
- Nothing kept here yet <sub>139</sub>
- ${count(d.whatsapp.files, "file")}, ${size(d.whatsapp.bytes)}, kept here for it, out of sight <sub>144</sub>
- ${count(chosen.length, "folder")} backed up here:  <sub>147</sub>
- Folders <sub>147</sub>
- Set up again <sub>151</sub>
- From before, ${count(earlier.held.files, "file")}, ${size(earlier.held.bytes)}, are still kept here. <sub>152</sub>
- Nothing from before is kept here. <sub>153</sub>
- Waiting for it to collect <sub>161</sub>
- Send files… <sub>170</sub>
- btn primary <sub>170</sub>
- Open their folder… <sub>176</sub>
- Remove this device… <sub>184</sub>
- Remove this guest… <sub>184</sub>
- btn ghost quiet-danger <sub>184</sub>
- Choose folders to back up… <sub>191</sub>
- Remove the earlier one… <sub>197</sub>
- btn ghost <sub>197</sub>
- Technical details <sub>208</sub>
- Identity <sub>211</sub>
- Direct <sub>212</sub>
- Not connected <sub>212</sub>
- Path <sub>212</sub>
- Relayed <sub>212</sub>
- Address <sub>213</sub>
- QUIC, end-to-end encrypted with this device's pinned key <sub>214</sub>
- Transport <sub>214</sub>

**D8.20** pop-up — `devices.js`, `openPhoneFolders`

- Back up ${d.name}'s folders <sub>230</sub>
- ${d.name} hasn't listed its folders here yet. On the phone, open Qurb, then Settings,  <sub>240</sub>
- Backups, Back up your folders, and allow it to read this phone's files. Its folders  <sub>241</sub>
- appear here after it next syncs. <sub>242</sub>
- actions end <sub>243</sub>
- Close <sub>244</sub>
- btn primary <sub>244</sub>
- The folders you choose are kept here, in ${d.name}'s Private Vault: over Wi-Fi, and files  <sub>251</sub>
- deleted on the phone stay here. <sub>252</sub>
- On ${d.name}, as it said ${when(answer.seen_at)} <sub>253</sub>
- group glass-frosted <sub>254</sub>
-  · ${count(f.kept_files, "file")} kept here <sub>262</sub>
- Open <sub>274</sub>
- btn small ghost <sub>274</sub>

**D8.21** pop-up — `devices.js`, `openBackup`

- Kept here from ${d.name}, including files deleted on the phone. <sub>328</sub>
- Files <sub>329</sub>
- Nothing kept here yet. It comes over Wi-Fi. <sub>341</sub>
- The first 500 of ${count(files.length, "file")}. <sub>358</sub>

**D8.22** pop-up — `devices.js`, `openGuestFolder`

- Folder kept for ${d.name} <sub>374</sub>
- actions end <sub>377</sub>

**D8.23** message — `devices.js`, `say`

- This computer stops trusting it: it can no longer connect or sync here. It keeps  <sub>475</sub>
- ${count(plan.waiting, "file")} waiting for it to collect will be cancelled. <sub>477</sub>
- It keeps this computer's private files. After this, it won't. <sub>478</sub>
- ${count(n, "file")} freed from this computer ${n === 1 ? "is" : "are"} kept only on  <sub>481</sub>
- Only on this computer: your other devices go on trusting it until you remove it there too. <sub>501</sub>
- Removed ${plan.name} <sub>518</sub>

### D9. Storage

**D9.1** page — `index.html`, `(top level)`

- Storage <sub>244</sub>
- What Qurb keeps on this computer, and how to make room without losing anything. <sub>245</sub>
- card glass-frosted <sub>250</sub>
- big-number num <sub>253</sub>
- quiet num <sub>254</sub>
- Free local space <sub>256</sub>
- btn primary <sub>256</sub>
- legend num <sub>260</sub>
- What is kept here <sub>263</sub>
- Files you can free <sub>266</sub>
- group glass-frosted <sub>271</sub>
- Limit how much space Qurb may use <sub>274</sub>
- item stack hidden <sub>279</sub>
- Storage limit <sub>280</sub>
- margin-top: 8px; align-items: center <sub>281</sub>
- flex: 1 <sub>282</sub>
- Saved <sub>283</sub>
- meta hidden <sub>283</sub>
- Save <sub>284</sub>
- btn small <sub>284</sub>

**D9.2** page — `settings.js`, `drawStorage`

- of ${size(st.limit)} allowed${st.over ? ", over the limit" : ""} <sub>29</sub>
- with no limit set <sub>29</sub>
- ${size(st.free_disk)} free on this disk <sub>30</sub>

**D9.3** page — `settings.js`, `showLimitLabel`

-  — ${Math.round((gib * GIB / disk) * 100)}% of this disk <sub>216</sub>
- ${gib} GB${share} <sub>217</sub>

**D9.4** page — `settings.js`, `saveLimit`

- In a file only you can read. Enough against other people using this computer;  <sub>247</sub>
- not against anyone who can read its disk. <sub>248</sub>
- In the system keystore, locked while you're logged out. Qurb opens it without  <sub>249</sub>
- asking while you're logged in. <sub>250</sub>
- Behind a passphrase only you know. Qurb asks for it when it starts, and nothing  <sub>251</sub>
- syncs until you type it — the only option that protects the key from someone with the disk and your login. <sub>252</sub>

### D10. Settings

**D10.1** page — `index.html`, `(top level)`

- Settings <sub>297</sub>
- How Qurb works on this computer. <sub>298</sub>
- This device <sub>302</sub>
- group glass-frosted <sub>303</sub>
- item stack <sub>304</sub>
- Name <sub>305</sub>
- What your other devices call this one. <sub>305</sub>
- Folder <sub>309</sub>
- Key protection <sub>312</sub>
- margin: 0 <sub>313</sub>
- item hidden <sub>315</sub>
- flex: 1 <sub>316</sub>
- margin-top: 0 <sub>317</sub>
- Current passphrase <sub>318</sub>
- New passphrase <sub>322</sub>
- At least 8 characters. Nobody can recover it for you; another of your devices can set this computer up again, with a code. <sub>324</sub>
- The same again <sub>327</sub>
- says warn hidden <sub>330</sub>
- Cancel <sub>332</sub>
- Set the passphrase <sub>333</sub>
- btn primary <sub>333</sub>
- Devices <sub>339</sub>
- Paired devices <sub>342</sub>
- Manage <sub>343</sub>
- btn small <sub>343</sub>
- Pairings and removals <sub>346</sub>
- margin-top: 6px <sub>347</sub>
- Storage <sub>351</sub>
- Files sent to this computer <sub>354</sub>
- Qurb, in your Downloads folder <sub>357</sub>
- off <sub>361</sub>
- to keep them inside your Qurb folder. <sub>361</sub>
- Open folder <sub>362</sub>
- Space and limit <sub>365</sub>
- Privacy <sub>370</sub>
- Keep new files private <sub>373</sub>
- Files added on this computer go to its Private Vault instead of to all your devices. <sub>374</sub>
- Notifications <sub>379</sub>
- Notify me <sub>382</sub>
- When a file is sent to this computer, when one you sent is collected, and when something fails. <sub>383</sub>
- Recovery <sub>388</sub>
- Where your key is safe <sub>392</sub>
- Recovery phrase <sub>396</sub>
- The 24 words that spell your key, if you want them; nobody needs to write them down. Anyone who sees them can read your files. <sub>397</sub>
- Show <sub>398</sub>
- item stack hidden <sub>400</sub>
- Appearance <sub>406</sub>
- Light, dark, or as your system is set. <sub>409</sub>
- Theme <sub>410</sub>
- System <sub>411</sub>
- Light <sub>412</sub>
- Dark <sub>413</sub>
- Follows your system's reduce-motion setting. <sub>417</sub>
- Motion <sub>417</sub>
- Full <sub>418</sub>
- Advanced <sub>422</sub>
- Rendezvous service <sub>425</sub>
- Where devices leave word that they are awake. It never sees your files. <sub>425</sub>
- Relay <sub>429</sub>
- Used only when devices can't reach each other directly. It carries encrypted data it can't read. <sub>429</sub>
- t reach each other directly. It carries encrypted data it can <sub>429</sub>
- Beside the rendezvous service — or off, or a relay of your own <sub>431</sub>
- Port <sub>434</sub>
- Zero means any. A fixed one helps if you forward it. <sub>434</sub>
- Save <sub>440</sub>
- btn primary small <sub>440</sub>
- Start when I log in <sub>443</sub>
- Without opening this window. Your phone can only reach this computer while Qurb runs here. <sub>443</sub>
- Activity <sub>447</sub>
- Everything this computer did, newest first. <sub>447</sub>
- Open <sub>448</sub>
- Identity <sub>451</sub>
- Version <sub>454</sub>
- Quit Qurb <sub>457</sub>
- Stops syncing until you open it again. Closing the window doesn't. <sub>457</sub>
- Quit <sub>458</sub>
- btn small quiet-danger <sub>458</sub>

**D10.2** page — `settings.js`, `key`

- Qurb uses ${size(st.used)} <sub>47</sub>
- ${size(fr.bytes)} can be freed <sub>48</sub>
- ${size(String(Math.max(0, limit - used)))} left of the limit <sub>49</sub>
- ${size(st.free_disk)} free on this disk <sub>49</sub>
- Sizes above are the files' own. On this disk Qurb takes ${size(st.used)} for all of it: compressed, and what files share kept once. <sub>54</sub>
- ${size(fr.bytes)} from ${count(fr.count, "file")}. They stay in Qurb, available from your other devices. <sub>57</sub>
- ${count(st.only_here, "file")} ${st.only_here === 1 ? "is" : "are"} only on this computer, so ${st.only_here === 1 ? "it stays" : "they stay"} until another device has ${st.only_here === 1 ? "it" : "them"}. <sub>59</sub>
- Nothing right now: everything here is either needed here or already freed. <sub>60</sub>
- Free local space <sub>64</sub>
- btn small <sub>64</sub>
- Shared <sub>68</sub>
- The largest ${FREEABLE_SHOWN} of ${count(fr.count, "file")}. Free local space, above, frees them all. <sub>77</sub>
- ${size(st.used)} of ${size(st.limit)}${st.over ? " — over the limit" : ""} <sub>81</sub>
- No limit set <sub>82</sub>

**D10.3** page — `settings.js`, `card`

- part card glass-frosted <sub>103</sub>
- part-size num <sub>108</sub>
- btn small <sub>114</sub>

**D10.4** page — `settings.js`, `sum`

- Your phones' backups <sub>127</sub>
- None yet <sub>128</sub>
- WhatsApp ${size(p.whatsapp)} <sub>131</sub>
- folders ${size(p.folders)} <sub>132</sub>
- Private Vault ${size(p.vault)} <sub>133</sub>
- A phone's backups come here once it keeps its Private Vault on this computer. <sub>135</sub>
- Open in Files <sub>136</sub>
- Shared files <sub>140</sub>
- ${size(parts.shared_here)} here <sub>141</sub>
- ${count(parts.shared_files, "file")}, on all your devices <sub>143</sub>
- ${count(st.evicted, "file")} freed here, fetched when opened <sub>144</sub>
- Every one of them on this computer <sub>144</sub>
- Guests' folders <sub>151</sub>
- None <sub>152</sub>
- ${g.name}: ${size(g.size)}, sealed <sub>154</sub>
- Nobody else keeps a folder here. A guest's is sealed: you can't open it. <sub>155</sub>
- Devices <sub>156</sub>
- Recently deleted <sub>160</sub>
- ${count(parts.deleted_files, "file")}, each kept 30 days <sub>162</sub>
- Nothing deleted lately <sub>162</sub>
- Open <sub>163</sub>

**D10.5** page — `settings.js`, `offer`

- btn small <sub>272</sub>
- Protect with a passphrase… <sub>277</sub>
- Change passphrase… <sub>278</sub>
- Use the keystore <sub>279</sub>
- Nothing yet <sub>283</sub>
- Paired <sub>288</sub>
- Removed <sub>288</sub>

**D10.6** page — `settings.js`, `protectForm`

- Set the passphrase <sub>304</sub>
- Use the keystore <sub>304</sub>
- The two new passphrases aren't the same. <sub>320</sub>

**D10.7** page — `settings.js`, `drawSettings`

- Only on this computer. Pair your phone: it can give the key to a new computer with a code, and keeps it in its Google backup. <sub>358</sub>
- On this computer and on ${others.join(", ")}. Any of them can give it to a new device with a code${s.phone_holds_key ? "; your phone also keeps it in its Google backup" : ". Pair a phone too: it keeps the key in its Google backup"}. <sub>359</sub>
- Full <sub>375</sub>
- Reduced <sub>375</sub>
- Err <sub>382</sub>
- Refused: ${at.Err} <sub>382</sub>
- Kept inside your Qurb folder. <sub>383</sub>
- Now: ${at.Ok} <sub>384</sub>
- None yet <sub>390</sub>

**D10.8** page — `settings.js`, `applyAtOnce`

- Qurb will start when you log in <sub>422</sub>
- Qurb won't start by itself <sub>422</sub>
- Files you add here now go to all your devices <sub>424</sub>
- Files you add here now go to your Private Vault <sub>424</sub>
- Notifications off <sub>426</sub>
- Notifications on <sub>426</sub>
- Show <sub>470</sub>
- Hide <sub>480</sub>

**D10.9** message — `settings.js`, `sum`

- Freed ${size(String(freed))} from ${count(n, "file")}. They stay in Qurb. <sub>180</sub>

**D10.10** message — `settings.js`, `protectForm`

- Key protection changed <sub>332</sub>

**D10.11** message — `settings.js`, `applyAtOnce`

- Settings saved <sub>440</sub>

### D11. Notifications

**D11.1** page — `main.rs`, `run`

- this key needs a passphrase <sub>91</sub>
- running the window: {e} <sub>213</sub>

**D11.2** notification — `notify.rs`, `worth_saying`

- sent to this device <sub>130</sub>
- ; saved to  <sub>135</sub>
- {name} sent you a file <sub>139</sub>
- Somebody sent you a file <sub>140</sub>
- {what} ¶ Saved to {} <sub>145</sub>
- {name} has {what} <sub>154</sub>
- {what} was collected <sub>155</sub>
- Qurb could not finish something <sub>160</sub>
- {what}: {why} <sub>162</sub>

**D11.3** notification — `notify.rs`, `still_running`

- qurb is still syncing <sub>223</sub>
- Open it again from the applications menu. To stop it, use Quit in Settings. <sub>224</sub>

**D11.4** notification — `notify.rs`, `a_file_somebody_sent_is_worth_saying`

- sent to this device <sub>246</sub>
- phone sent you a file <sub>249</sub>

**D11.5** notification — `notify.rs`, `a_file_saved_to_downloads_says_where`

- sent to this device; saved to /data/Downloads/qurb/photo.jpg <sub>260</sub>
- phone sent you a file <sub>263</sub>
- photo.jpg ¶ Saved to /data/Downloads/qurb <sub>264</sub>

**D11.6** notification — `notify.rs`, `a_delivery_from_a_device_with_no_name_still_reads`

- sent to this device <sub>271</sub>
- Somebody sent you a file <sub>273</sub>

**D11.7** notification — `notify.rs`, `failures_are_worth_saying_and_carry_the_reason`

- permission denied <sub>286</sub>

**D11.8** notification — `notify.rs`, `a_collection_closes_the_loop_by_name`

- phone has tickets.pdf <sub>295</sub>

### D12. Tray menu

**D12.1** menu — `ui.rs`, `build`

- showing a tray icon for {} <sub>81</sub>

**D12.2** menu — `ui.rs`, `menu`

- Qurb — {} <sub>89</sub>
- {} file{} · {} · {} <sub>94</sub>
- Recently synced <sub>107</sub>
- ⚠ {problem} <sub>119</sub>
- Open folder <sub>123</sub>

**D12.3** menu — `ui.rs`, `run_headless`

- qurb: {} — {} file{}, {} <sub>194</sub>
- the daemon stopped <sub>222</sub>

**D12.4** menu — `ui.rs`, `tooltip`

- Qurb — {} ¶ {} <sub>241</sub>

**D12.5** menu — `ui.rs`, `devices`

- no paired devices <sub>246</sub>
- {n} device{} · none reachable <sub>247</sub>
- {n} device{} <sub>248</sub>
- {r} of {n} devices reachable <sub>249</sub>

**D12.6** menu — `ui.rs`, `ago`

- just now <sub>275</sub>
- {}m ago <sub>279</sub>
- {}h ago <sub>280</sub>
- {}d ago <sub>281</sub>

**D12.7** menu — `ui.rs`, `human`

- {bytes} B <sub>294</sub>
- {value:.1} {} <sub>296</sub>

**D12.8** menu — `ui.rs`, `a_long_path_keeps_its_filename`

- still {} chars <sub>308</sub>
- lost the filename: {short} <sub>309</sub>

**D12.9** menu — `ui.rs`, `device_counts_distinguish_unpaired_from_unreachable`

- no paired devices <sub>322</sub>
- 2 devices · none reachable <sub>326</sub>
- 2 devices <sub>329</sub>
- 1 of 2 devices reachable <sub>332</sub>

**D12.10** menu — `ui.rs`, `sizes_read_as_people_write_them`

- 1.5 KB <sub>339</sub>
- 4.7 MB <sub>340</sub>

### D13. Messages from the engine

**D13.1** message — `commands.rs`, `situation`

- this folder has a key that could not be opened <sub>245</sub>

**D13.2** message — `commands.rs`, `unlock`

- that passphrase does not open this key <sub>264</sub>

**D13.3** message — `commands.rs`, `protect_key`

- a passphrase needs at least 8 characters <sub>329</sub>
- the current passphrase is not right <sub>334</sub>

**D13.4** message — `commands.rs`, `bytes_of`

- choose how much space qurb may use <sub>402</sub>

**D13.5** message — `commands.rs`, `enrol_device`

- those are not 24 valid words — check the spelling and the order <sub>419</sub>

**D13.6** message — `commands.rs`, `join_new_device`

- that code has expired — ask the other device for a new one <sub>450</sub>
- the other device did not give its key — the code may already have been used, or that device needs updating <sub>453</sub>

**D13.7** message — `commands.rs`, `reveal_phrase`

- this key is protected by a passphrase; use `qurb status` in a terminal <sub>475</sub>

**D13.8** message — `commands.rs`, `settings`

- qurb  <sub>516</sub>
- window {} · engine  <sub>516</sub>

**D13.9** message — `commands.rs`, `save_settings`

- a device needs a name — it is what the others will call it <sub>551</sub>

**D13.10** message — `commands.rs`, `open_downloads`

- files sent here are kept in the folder <sub>575</sub>

**D13.11** message — `commands.rs`, `show_received`

- that is no longer in the history <sub>592</sub>
- no downloads folder <sub>594</sub>

**D13.12** message — `commands.rs`, `folder_to_show`

- ; saved to  <sub>605</sub>
- that file was not saved to Downloads <sub>607</sub>
- that file has no folder <sub>608</sub>
- that folder is not there any more <sub>610</sub>
- that file is no longer in the downloads folder <sub>613</sub>

**D13.13** message — `commands.rs`, `reveal`

- could not open the file manager: {e} <sub>621</sub>

**D13.14** message — `commands.rs`, `recipient`

- no paired device {to} <sub>732</sub>
- more than one device is called {to} <sub>733</sub>

**D13.15** message — `commands.rs`, `start_pairing`

- watches until dropped <sub>900</sub>

**D13.16** message — `commands.rs`, `answer_pairing`

- no device is asking any more <sub>961</sub>

**D13.17** message — `commands.rs`, `typed`

- that is not a pairing code — type the six digits, or the whole code <sub>979</sub>

**D13.18** message — `commands.rs`, `join_device`

- that code has expired — ask the other device for a new one <sub>1021</sub>

**D13.19** message — `commands.rs`, `summary`

- this device is not set up yet <sub>1052</sub>

**D13.20** message — `commands.rs`, `as_entry`

- only here <sub>1192</sub>

**D13.21** message — `commands.rs`, `on_disk`

- that is not a file in qurb <sub>1354</sub>
- that file is not in qurb any more <sub>1359</sub>
- that file is not on this device — keep it here first <sub>1362</sub>
- no device this one syncs with has that file any more <sub>1365</sub>

**D13.22** message — `commands.rs`, `base64`

- chunks of at most three <sub>1435</sub>

**D13.23** message — `commands.rs`, `own_phone`

- {} is another person's device <sub>1482</sub>

**D13.24** message — `commands.rs`, `owner_folder_open`

- your folder is locked: open it in Files, and approve on your phone <sub>1493</sub>

**D13.25** message — `commands.rs`, `choose_phone_folder`

- {} has no folder {folder} <sub>1538</sub>

**D13.26** message — `commands.rs`, `phone_backup`

- that is not a folder of the phone <sub>1560</sub>

**D13.27** message — `commands.rs`, `open_backed_up`

- that is not a file of the backup <sub>1580</sub>
- that file has no name <sub>1582</sub>

**D13.28** message — `commands.rs`, `guest_person`

- {} is not a guest of this computer <sub>1623</sub>

**D13.29** message — `commands.rs`, `guest_folder`

- the folder has just locked itself <sub>1653</sub>

**D13.30** message — `commands.rs`, `open_guest_file`

- the folder is locked <sub>1680</sub>
- that file is not in the folder <sub>1685</sub>
- that file has no name <sub>1686</sub>

**D13.31** message — `commands.rs`, `person_of`

- no such person <sub>1764</sub>

**D13.32** message — `commands.rs`, `owner_phones`

- {}, earlier <sub>1803</sub>

**D13.33** message — `commands.rs`, `person_folder`

- the folder has just locked itself <sub>1928</sub>

**D13.34** message — `commands.rs`, `open_person_file`

- the folder is locked <sub>2007</sub>
- which phone's file? <sub>2011</sub>
- that is not a file of the folder <sub>2014</sub>
- that file is not in the folder <sub>2019</sub>
- {name} is {}, more than this computer can open in memory ({} free). Open it on the phone it came from. <sub>2040</sub>
- that file has no folder <sub>2046</sub>

**D13.35** message — `commands.rs`, `open_sealed`

- that file is not in the folder <sub>2081</sub>
- that file has no name <sub>2082</sub>

**D13.36** message — `commands.rs`, `show_file`

- that file has no folder <sub>2103</sub>

**D13.37** message — `commands.rs`, `sharing`

- This computer <sub>2314</sub>

**D13.38** message — `commands.rs`, `set_sharing`

- not a device: {hex} <sub>2340</sub>

**D13.39** message — `commands.rs`, `conflicts`

- this computer <sub>2409</sub>
- another device <sub>2420</sub>

**D13.40** message — `commands.rs`, `settle_conflict`

- keep this, other or both, not {keep} <sub>2444</sub>
- other version <sub>2460</sub>

**D13.41** message — `commands.rs`, `recently_deleted`

- this computer <sub>2498</sub>

**D13.42** message — `commands.rs`, `paired_device`

- no paired device {text} <sub>2546</sub>

**D13.43** message — `commands.rs`, `set_limit`

- that is not a number of bytes <sub>2598</sub>

**D13.44** message — `commands.rs`, `detail`

- sent to this device; saved to {} <sub>2629</sub>

**D13.45** message — `commands.rs`, `a_typed_folder_is_found_from_the_home_folder`

- nothing typed is not the home folder <sub>2643</sub>

**D13.46** message — `commands.rs`, `an_entry_that_is_not_a_download_opens_nothing`

- sent to this device <sub>2683</sub>

**D13.47** message — `commands.rs`, `set_starts_at_login`

- there is no config directory to start from <sub>2703</sub>

### D14. Shared components

**D14.1** page — `core.js`, `calm`

- (prefers-reduced-motion: reduce) <sub>56</sub>

**D14.2** page — `core.js`, `when`

- just now <sub>75</sub>

**D14.3** page — `core.js`, `plural`

- ${n} ${unit}${n === 1 ? "" : "s"} ago <sub>82</sub>

**D14.4** page — `core.js`, `count`

- ${n.toLocaleString()} ${noun}${n === 1 ? "" : "s"} <sub>86</sub>

**D14.5** page — `core.js`, `duration`

- ${Math.round(seconds / 60)} min <sub>93</sub>

**D14.6** page — `core.js`, `kindOf`

- Archive <sub>123</sub>
- Audio <sub>123</sub>
- Image <sub>123</sub>
- Video <sub>123</sub>
- Code <sub>124</sub>
- Document <sub>124</sub>
- File <sub>124</sub>

**D14.7** page — `core.js`, `stateOf`

- On this device <sub>134</sub>
- Available elsewhere <sub>135</sub>
- Only copy here <sub>136</sub>
- only here <sub>136</sub>
- On no device <sub>138</sub>

**D14.8** page — `core.js`, `stateTag`

- state ${s.cls} <sub>145</sub>

**D14.9** page — `core.js`, `row`

- row ${cls} <sub>157</sub>
- Enter <sub>180</sub>

**D14.10** page — `core.js`, `empty`

- empty small <sub>188</sub>

**D14.11** page — `core.js`, `attention`

- attention error <sub>203</sub>

**D14.12** page — `core.js`, `onKey`

- Escape <sub>241</sub>

**D14.13** page — `core.js`, `menu`

- menu glass-elevated <sub>281</sub>
- ${Math.min(window.innerWidth - width - 12, Math.max(12, r.right - width))}px <sub>295</sub>
- ${Math.max(12, r.top - box.offsetHeight - 6)}px <sub>298</sub>
- ${below}px <sub>298</sub>

**D14.14** page — `core.js`, `toast`

- toast glass-elevated <sub>310</sub>
- toast warn glass-elevated <sub>310</sub>

**D14.15** page — `core.js`, `otherDevice`

- another device <sub>351</sub>

**D14.16** page — `icons.js`, `(top level)`

- <svg xmlns="http://www.w3.org/2000/svg" style="display:none" aria-hidden="true"><symbol id="i-house" viewBox="0 0 24 24"><path d="M15 21v-8a1 1 0 0 0-1-1h-4a1 1 0 0 0-1 1v8" /> <path d="M3 10a2 2 0 0 1 .709-1.528l7-6a2 2 0 0 1 2.582 0l7 6A2 2 0 0 1 21 10v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></symbol><symbol id="i-folder" viewBox="0 0 24 24"><path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" /></symbol><symbol id="i-folder-open" viewBox="0 0 24 24"><path d="m6 14 1.5-2.9A2 2 0 0 1 9.24 10H20a2 2 0 0 1 1.94 2.5l-1.54 6a2 2 0 0 1-1.95 1.5H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H18a2 2 0 0 1 2 2v2" /></symbol><symbol id="i-file" viewBox="0 0 24 24"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /></symbol><symbol id="i-file-text" viewBox="0 0 24 24"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M10 9H8" /> <path d="M16 13H8" /> <path d="M16 17H8" /></symbol><symbol id="i-file-image" viewBox="0 0 24 24"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <circle cx="10" cy="12" r="2" /> <path d="m20 17-1.296-1.296a2.41 2.41 0 0 0-3.408 0L9 22" /></symbol><symbol id="i-file-video" viewBox="0 0 24 24"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M15.033 13.44a.647.647 0 0 1 0 1.12l-4.065 2.352a.645.645 0 0 1-.968-.56v-4.704a.645.645 0 0 1 .967-.56z" /></symbol><symbol id="i-file-audio" viewBox="0 0 24 24"><path d="M4 6.835V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.706.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2h-.343" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M2 19a2 2 0 0 1 4 0v1a2 2 0 0 1-4 0v-4a6 6 0 0 1 12 0v4a2 2 0 0 1-4 0v-1a2 2 0 0 1 4 0" /></symbol><symbol id="i-file-archive" viewBox="0 0 24 24"><path d="M13.659 22H18a2 2 0 0 0 2-2V8a2.4 2.4 0 0 0-.706-1.706l-3.588-3.588A2.4 2.4 0 0 0 14 2H6a2 2 0 0 0-2 2v11.5" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M8 12v-1" /> <path d="M8 18v-2" /> <path d="M8 7V6" /> <circle cx="8" cy="20" r="2" /></symbol><symbol id="i-file-code" viewBox="0 0 24 24"><path d="M6 22a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h8a2.4 2.4 0 0 1 1.704.706l3.588 3.588A2.4 2.4 0 0 1 20 8v12a2 2 0 0 1-2 2z" /> <path d="M14 2v5a1 1 0 0 0 1 1h5" /> <path d="M10 12.5 8 15l2 2.5" /> <path d="m14 12.5 2 2.5-2 2.5" /></symbol><symbol id="i-monitor-smartphone" viewBox="0 0 24 24"><path d="M18 8V6a2 2 0 0 0-2-2H4a2 2 0 0 0-2 2v7a2 2 0 0 0 2 2h8" /> <path d="M10 19v-3.96 3.15" /> <path d="M7 19h5" /> <rect width="6" height="10" x="16" y="12" rx="2" /></symbol><symbol id="i-hard-drive" viewBox="0 0 24 24"><path d="M10 16h.01" /> <path d="M2.212 11.577a2 2 0 0 0-.212.896V18a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-5.527a2 2 0 0 0-.212-.896L18.55 5.11A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11z" /> <path d="M21.946 12.013H2.054" /> <path d="M6 16h.01" /></symbol><symbol id="i-settings" viewBox="0 0 24 24"><path d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915" /> <circle cx="12" cy="12" r="3" /></symbol><symbol id="i-send" viewBox="0 0 24 24"><path d="M14.536 21.686a.5.5 0 0 0 .937-.024l6.5-19a.496.496 0 0 0-.635-.635l-19 6.5a.5.5 0 0 0-.024.937l7.93 3.18a2 2 0 0 1 1.112 1.11z" /> <path d="m21.854 2.147-10.94 10.939" /></symbol><symbol id="i-arrow-up-down" viewBox="0 0 24 24"><path d="m21 16-4 4-4-4" /> <path d="M17 20V4" /> <path d="m3 8 4-4 4 4" /> <path d="M7 4v16" /></symbol><symbol id="i-lock-keyhole" viewBox="0 0 24 24"><circle cx="12" cy="16" r="1" /> <rect x="3" y="10" width="18" height="12" rx="2" /> <path d="M7 10V7a5 5 0 0 1 10 0v3" /></symbol><symbol id="i-search" viewBox="0 0 24 24"><path d="m21 21-4.34-4.34" /> <circle cx="11" cy="11" r="8" /></symbol><symbol id="i-arrow-left" viewBox="0 0 24 24"><path d="m12 19-7-7 7-7" /> <path d="M19 12H5" /></symbol><symbol id="i-ellipsis" viewBox="0 0 24 24"><circle cx="12" cy="12" r="1" /> <circle cx="19" cy="12" r="1" /> <circle cx="5" cy="12" r="1" /></symbol><symbol id="i-download" viewBox="0 0 24 24"><path d="M12 15V3" /> <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /> <path d="m7 10 5 5 5-5" /></symbol><symbol id="i-cloud-off" viewBox="0 0 24 24"><path d="M10.94 5.274A7 7 0 0 1 15.71 10h1.79a4.5 4.5 0 0 1 4.222 6.057" /> <path d="M18.796 18.81A4.5 4.5 0 0 1 17.5 19H9A7 7 0 0 1 5.79 5.78" /> <path d="m2 2 20 20" /></symbol><symbol id="i-cloud" viewBox="0 0 24 24"><path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z" /></symbol><symbol id="i-check" viewBox="0 0 24 24"><path d="M20 6 9 17l-5-5" /></symbol><symbol id="i-git-compare" viewBox="0 0 24 24"><circle cx="18" cy="18" r="3" /> <circle cx="6" cy="6" r="3" /> <path d="M13 6h3a2 2 0 0 1 2 2v7" /> <path d="M11 18H8a2 2 0 0 1-2-2V9" /></symbol><symbol id="i-trash-2" viewBox="0 0 24 24"><path d="M10 11v6" /> <path d="M14 11v6" /> <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" /> <path d="M3 6h18" /> <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" /></symbol><symbol id="i-clock" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" /> <path d="M12 6v6l4 2" /></symbol><symbol id="i-rotate-ccw" viewBox="0 0 24 24"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /> <path d="M3 3v5h5" /></symbol><symbol id="i-triangle-alert" viewBox="0 0 24 24"><path d="m21.73 18-8-14a2 2 0 0 0-3.48 0l-8 14A2 2 0 0 0 4 21h16a2 2 0 0 0 1.73-3" /> <path d="M12 9v4" /> <path d="M12 17h.01" /></symbol><symbol id="i-circle-alert" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" /> <line x1="12" x2="12" y1="8" y2="12" /> <line x1="12" x2="12.01" y1="16" y2="16" /></symbol><symbol id="i-x" viewBox="0 0 24 24"><path d="M18 6 6 18" /> <path d="m6 6 12 12" /></symbol><symbol id="i-chevron-right" viewBox="0 0 24 24"><path d="m9 18 6-6-6-6" /></symbol><symbol id="i-chevron-down" viewBox="0 0 24 24"><path d="m6 9 6 6 6-6" /></symbol><symbol id="i-laptop" viewBox="0 0 24 24"><path d="M18 5a2 2 0 0 1 2 2v8.526a2 2 0 0 0 .212.897l1.068 2.127a1 1 0 0 1-.9 1.45H3.62a1 1 0 0 1-.9-1.45l1.068-2.127A2 2 0 0 0 4 15.526V7a2 2 0 0 1 2-2z" /> <path d="M20.054 15.987H3.946" /></symbol><symbol id="i-smartphone" viewBox="0 0 24 24"><rect width="14" height="20" x="5" y="2" rx="2" ry="2" /> <path d="M12 18h.01" /></symbol><symbol id="i-monitor" viewBox="0 0 24 24"><rect width="20" height="14" x="2" y="3" rx="2" /> <line x1="8" x2="16" y1="21" y2="21" /> <line x1="12" x2="12" y1="17" y2="21" /></symbol><symbol id="i-upload" viewBox="0 0 24 24"><path d="M12 3v12" /> <path d="m17 8-5-5-5 5" /> <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" /></symbol><symbol id="i-external-link" viewBox="0 0 24 24"><path d="M15 3h6v6" /> <path d="M10 14 21 3" /> <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" /></symbol><symbol id="i-folder-search" viewBox="0 0 24 24"><path d="M10.7 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.69.9l.81 1.2a2 2 0 0 0 1.67.9H20a2 2 0 0 1 2 2v4.1" /> <path d="m21 21-1.9-1.9" /> <circle cx="17" cy="17" r="3" /></symbol><symbol id="i-key-round" viewBox="0 0 24 24"><path d="M2.586 17.414A2 2 0 0 0 2 18.828V21a1 1 0 0 0 1 1h3a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h1a1 1 0 0 0 1-1v-1a1 1 0 0 1 1-1h.172a2 2 0 0 0 1.414-.586l.814-.814a6.5 6.5 0 1 0-4-4z" /> <circle cx="16.5" cy="7.5" r=".5" fill="currentColor" /></symbol><symbol id="i-bell" viewBox="0 0 24 24"><path d="M10.268 21a2 2 0 0 0 3.464 0" /> <path d="M3.262 15.326A1 1 0 0 0 4 17h16a1 1 0 0 0 .74-1.673C19.41 13.956 18 12.499 18 8A6 6 0 0 0 6 8c0 4.499-1.411 5.956-2.738 7.326" /></symbol><symbol id="i-palette" viewBox="0 0 24 24"><path d="M12 22a1 1 0 0 1 0-20 10 9 0 0 1 10 9 5 5 0 0 1-5 5h-2.25a1.75 1.75 0 0 0-1.4 2.8l.3.4a1.75 1.75 0 0 1-1.4 2.8z" /> <circle cx="13.5" cy="6.5" r=".5" fill="currentColor" /> <circle cx="17.5" cy="10.5" r=".5" fill="currentColor" /> <circle cx="6.5" cy="12.5" r=".5" fill="currentColor" /> <circle cx="8.5" cy="7.5" r=".5" fill="currentColor" /></symbol><symbol id="i-sliders-horizontal" viewBox="0 0 24 24"><path d="M10 5H3" /> <path d="M12 19H3" /> <path d="M14 3v4" /> <path d="M16 17v4" /> <path d="M21 12h-9" /> <path d="M21 19h-5" /> <path d="M21 5h-7" /> <path d="M8 10v4" /> <path d="M8 12H3" /></symbol><symbol id="i-shield" viewBox="0 0 24 24"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" /></symbol><symbol id="i-qr-code" viewBox="0 0 24 24"><rect width="5" height="5" x="3" y="3" rx="1" /> <rect width="5" height="5" x="16" y="3" rx="1" /> <rect width="5" height="5" x="3" y="16" rx="1" /> <path d="M21 16h-3a2 2 0 0 0-2 2v3" /> <path d="M21 21v.01" /> <path d="M12 7v3a2 2 0 0 1-2 2H7" /> <path d="M3 12h.01" /> <path d="M12 3h.01" /> <path d="M12 16v.01" /> <path d="M16 12h1" /> <path d="M21 12v.01" /> <path d="M12 21v-1" /></symbol><symbol id="i-keyboard" viewBox="0 0 24 24"><path d="M10 8h.01" /> <path d="M12 12h.01" /> <path d="M14 8h.01" /> <path d="M16 12h.01" /> <path d="M18 8h.01" /> <path d="M6 8h.01" /> <path d="M7 16h10" /> <path d="M8 12h.01" /> <rect width="20" height="16" x="2" y="4" rx="2" /></symbol><symbol id="i-power" viewBox="0 0 24 24"><path d="M12 2v10" /> <path d="M18.4 6.6a9 9 0 1 1-12.77.04" /></symbol><symbol id="i-refresh-cw" viewBox="0 0 24 24"><path d="M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8" /> <path d="M21 3v5h-5" /> <path d="M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16" /> <path d="M8 16H3v5" /></symbol><symbol id="i-wifi-off" viewBox="0 0 24 24"><path d="M12 20h.01" /> <path d="M8.5 16.429a5 5 0 0 1 7 0" /> <path d="M5 12.859a10 10 0 0 1 5.17-2.69" /> <path d="M19 12.859a10 10 0 0 0-2.007-1.523" /> <path d="M2 8.82a15 15 0 0 1 4.177-2.643" /> <path d="M22 8.82a15 15 0 0 0-11.288-3.764" /> <path d="m2 2 20 20" /></symbol><symbol id="i-pencil" viewBox="0 0 24 24"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z" /> <path d="m15 5 4 4" /></symbol><symbol id="i-copy" viewBox="0 0 24 24"><rect width="14" height="14" x="8" y="8" rx="2" ry="2" /> <path d="M4 16c-1.1 0-2-.9-2-2V4c0-1.1.9-2 2-2h10c1.1 0 2 .9 2 2" /></symbol><symbol id="i-arrow-right" viewBox="0 0 24 24"><path d="M5 12h14" /> <path d="m12 5 7 7-7 7" /></symbol><symbol id="i-circle-check" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" /> <path d="m16 9-5.5 5.5L8 12" /></symbol><symbol id="i-info" viewBox="0 0 24 24"><circle cx="12" cy="12" r="10" /> <path d="M12 16v-4" /> <path d="M12 8h.01" /></symbol><symbol id="i-plus" viewBox="0 0 24 24"><path d="M5 12h14" /> <path d="M12 5v14" /></symbol><symbol id="i-shield-check" viewBox="0 0 24 24"><path d="M20 13c0 5-3.5 7.5-7.66 8.95a1 1 0 0 1-.67-.01C7.5 20.5 4 18 4 13V6a1 1 0 0 1 1-1c2 0 4.5-1.2 6.24-2.72a1.17 1.17 0 0 1 1.52 0C14.51 3.81 17 5 19 5a1 1 0 0 1 1 1z" /> <path d="m9 12 2 2 4-4" /></symbol><symbol id="i-history" viewBox="0 0 24 24"><path d="M3 12a9 9 0 1 0 9-9 9.75 9.75 0 0 0-6.74 2.74L3 8" /> <path d="M3 3v5h5" /> <path d="M12 7v5l4 2" /></symbol><symbol id="i-user-plus" viewBox="0 0 24 24"><path d="M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2" /> <circle cx="9" cy="7" r="4" /> <line x1="19" x2="19" y1="8" y2="14" /> <line x1="22" x2="16" y1="11" y2="11" /></symbol></svg> <sub>10</sub>

**D14.17** page — `theme.js`, `(top level)`

- (prefers-color-scheme: dark) <sub>8</sub>

**D14.18** pop-up — `core.js`, `sheet`

- sheet glass-elevated <sub>222</sub>
- sheet wide glass-elevated <sub>222</sub>
- icon-btn close <sub>225</sub>

**D14.19** pop-up — `core.js`, `panel`

- panel glass-elevated ${cls} <sub>254</sub>
- icon-btn close <sub>255</sub>


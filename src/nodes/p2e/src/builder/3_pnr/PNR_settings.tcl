## Copyright (c) 2020-2023 by XEPIC Co., Ltd. ##

#*********************Example******************#
#
ena_auto_MPAR 0
#ena_auto_PRT_opt 1
#ena_pre_opt_cons_check_stop_PNR 0
#MPAR_mode "hold_meet"
#MPAR_ignore_WNS_THR -2.0
#MPAR_ignore_WHS_THR -0.1
#mpar_cluster_submit "bsub"
#mpar_cluster_type "LSF"
#user_pnr_strategy {part_b0_f0=MPAR_xxx,MPAR_yyy part_b0_f1=MPAR part_b0_f3=runtimeopt}
#REMOVE_HLUTNM 1
#REMOVE_SOFT_HLUTNM 1
#REMOVE_IS_DUT 1
#REMOVE_MARK_DEBUG 1
#REMOVE_DONT_TOUCH 1
#clock_root_deskew 1
#clock_region_deskew 1
#
#************************************************#

# Disable pre-optimization constraints check
# This allows PNR to continue even if some constraints reference non-existent objects
# (e.g., debug ports that don't exist when using WithNoDebug config)
ena_pre_opt_cons_check_stop_PNR 0
